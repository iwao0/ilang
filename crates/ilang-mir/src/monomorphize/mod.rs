//! AST monomorphization pass: turn each generic class instantiation
//! (`Box<i64>`) into a concrete non-generic class (`Box<i64>` mangled
//! into a unique class name) by cloning the declaration and
//! substituting the type parameters throughout fields, method
//! signatures, and method bodies.
//!
//! After this pass runs, the program contains zero `Type::Generic`,
//! `Type::TypeVar`, or `ExprKind::New { type_args: !empty }` nodes —
//! the JIT pipeline can then proceed unchanged.
//!
//! Strategy: walk the program collecting `(class_name, [arg types])`
//! instantiation seeds, iteratively expand by substituting and
//! re-walking the cloned method bodies until a fixed point is reached
//! (a method body may reference further generic types). Replace the
//! original generic class declarations with the synthesized concrete
//! ones.

use std::collections::HashSet;

use ilang_ast::{
    Symbol, Type,
};

mod class;
mod enums;
mod fns;
mod methods;
mod walk;

pub use class::monomorphize;
pub use enums::monomorphize_enums;
pub use fns::monomorphize_fns;
pub use methods::monomorphize_methods;

/// The unique key of a monomorphization request: class name + concrete
/// type arguments. We don't derive Hash on `Type`, so the worklist
/// uses the rendered mangled string for dedup; the args are kept
/// separately for substitution.
#[derive(Clone, Debug)]
pub(super) struct InstKey {
    pub(super) class: Symbol,
    pub(super) args: Vec<Type>,
}

pub(super) fn mangle(class: &str, args: &[Type]) -> Symbol {
    // Embed the concrete args in the class name. The result is not a
    // valid identifier (contains `<`, `,`, `>`), but class names live
    // as opaque strings throughout the JIT — we never re-parse them —
    // so this is safe and easy to debug.
    let mut s = class.to_string();
    s.push('<');
    for (i, a) in args.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        s.push_str(&a.to_string());
    }
    s.push('>');
    Symbol::intern(&s)
}

impl InstKey {
    pub(super) fn mangled(&self) -> Symbol {
        mangle(self.class.as_str(), &self.args)
    }
}

// Thread-local set of generic-enum names. Populated at the top of
// `monomorphize()`; consulted by `rewrite_type` to decide whether a
// `Type::Generic { base, args }` should be collapsed to a mangled
// `Object` (class case) or left as-is (enum case — JIT errors out
// later with a clear "generic enum + JIT unsupported" message).
thread_local! {
    static GENERIC_ENUM_NAMES: std::cell::RefCell<HashSet<Symbol>> =
        std::cell::RefCell::new(HashSet::new());
}

//
// Generic fns don't carry explicit `<T>` syntax at call sites — the
// type checker infers them from the arg types and stashes the result
// in `call_type_args` keyed by the call expression's span. This pass
// consumes that side table to:
//
// 1. Synthesize one concrete `FnDecl` per (generic_fn, concrete args)
//    pair actually used in the program.
// 2. Rewrite each Call's callee from the generic name to the mangled
//    concrete name.
// 3. Drop the generic templates from the output.
//
// **Limitation**: only call sites whose recorded type args are fully
// concrete (no `TypeVar`) get rewritten. A generic fn called from
// inside another generic context (e.g. a still-generic class method
// that survived class monomorphization for some reason) leaves a
// dangling reference; the JIT then errors with "unknown function".
// In practice class monomorphization runs first so all class-method
// bodies are concrete by the time we get here.

//
// Runs after `monomorphize` (which handles classes). Generic enums
// require a per-(name, args) concrete `EnumDecl` so the JIT can pin
// down each variant's payload size. The class pass deliberately
// leaves `Type::Generic { Enum, [..] }` alone; this pass:
//
// 1. Catalogs generic enums (user-defined + the built-in `Result`).
// 2. Seeds a worklist from every concrete instantiation it sees —
//    both `Type::Generic` refs in field/param/return slots AND
//    `EnumCtor` calls (looked up via the type-checker's side table).
// 3. Synthesizes concrete `EnumDecl`s by substituting each variant's
//    payload types.
// 4. Rewrites the rest of the program: `Type::Generic { Enum, ... }`
//    → `Type::Object(mangled)`, `EnumCtor.enum_name` → mangled.
// 5. Drops the original generic enum declarations from the output.

