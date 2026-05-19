use std::collections::{HashMap, HashSet};

use ilang_ast::{
    Expr, ExprKind, Span, Symbol, Type,
};

use crate::ops::{assignable, int_literal_fits};

mod builtins;
mod check;
mod decls;
mod expr;
mod extern_c;
mod free_vars;
mod match_;
mod method;
mod sigs;
mod stmt;
mod utils;
mod walks;

use free_vars::collect_fn_expr_free_vars;
use sigs::*;
use walks::{block_uses_this_directly, collect_this_field_assignments, refine_returns};

/// Check whether a value expression can be assigned to a binding of type
/// `target`. In addition to the normal `assignable` rule, an integer
/// literal (or its unary negation) infers into any integer type whose
/// range it fits — this is what lets `let x: u8 = 5` work even though
/// the literal's natural type is i64.
/// `if` の枝合流専用の判定: 値式が **素の数値リテラル** (整数/浮動小数、
/// 任意で単項 `-`) で、`target` 型に収まるかどうか。`assignable` を経由
/// しないので i64 値→f64 のような暗黙拡張は通さない。
pub(super) fn numeric_literal_fits(value: &Expr, target: &Type) -> bool {
    match &value.kind {
        ExprKind::Int(n) => {
            if target.is_int() {
                int_literal_fits(*n, target)
            } else {
                target.is_float()
            }
        }
        ExprKind::Unary { op: ilang_ast::UnOp::Neg, expr: inner } => {
            if let ExprKind::Int(n) = &inner.kind {
                if target.is_int() {
                    n.checked_neg().is_some_and(|v| int_literal_fits(v, target))
                } else {
                    target.is_float()
                }
            } else if matches!(inner.kind, ExprKind::Float(_)) {
                target.is_float()
            } else {
                false
            }
        }
        ExprKind::Float(_) => target.is_float(),
        _ => false,
    }
}

pub(super) fn literal_assignable(value: &Expr, vt: &Type, target: &Type) -> bool {
    literal_assignable_with(value, vt, target, &|_, _| false)
}

/// Same as `literal_assignable` but folds in a class-subtype test at
/// each Object-vs-Object position (the closure's contract is: "is the
/// first class a subclass of the second?"). Lets the recursive
/// composite-type cases (Array element, Tuple element, Optional inner)
/// upcast a child class to its parent inside literal contexts the way
/// the top-level binding paths already do.
pub(super) fn literal_assignable_with<F>(
    value: &Expr,
    vt: &Type,
    target: &Type,
    is_sub: &F,
) -> bool
where
    F: Fn(Symbol, Symbol) -> bool,
{
    // Integer literals get a fits-in-target check FIRST: the
    // general `assignable` rule allows int↔int narrowing without
    // a cast, which is fine for runtime-typed values but wrong
    // for compile-time literals where we can detect overflow.
    // `let a: i8 = 999` would otherwise be silently accepted and
    // wrap to -25 at runtime.
    if let ExprKind::Int(n) = &value.kind {
        if target.is_int() {
            return int_literal_fits(*n, target);
        }
        if target.is_float() {
            return true;
        }
    }
    if let ExprKind::Unary { op: ilang_ast::UnOp::Neg, expr: inner } = &value.kind {
        if let ExprKind::Int(n) = &inner.kind {
            if target.is_int() {
                return n.checked_neg().is_some_and(|v| int_literal_fits(v, target));
            }
            if target.is_float() {
                return true;
            }
        }
    }
    if assignable(vt, target) {
        return true;
    }
    // Object-vs-Object subtype: `B extends A` ⇒ B can flow into an
    // A slot. Mirrors `assignable_obj` at the leaves.
    if let (Type::Object(c), Type::Object(p)) = (vt, target) {
        if is_sub(*c, *p) {
            return true;
        }
    }
    // `let x: T? = literal` — auto-wrap. The literal is assignable to T?
    // iff it's assignable to the inner T (with literal coercions).
    if let Type::Optional(inner) = target {
        // `none` itself: vt = Optional<Any>, handled by `assignable`.
        // For `some(x)` we have to descend into the wrapped expression
        // so the recursive composite cases see the actual literal
        // (e.g. `some([new B()])` against `A[]?` matches the array-
        // literal branch with target=A[]).
        if let Type::Optional(vt_inner) = vt {
            if let ExprKind::Some(inner_expr) = &value.kind {
                return literal_assignable_with(inner_expr, vt_inner, inner, is_sub);
            }
            return literal_assignable_with(value, vt_inner, inner, is_sub);
        }
        return literal_assignable_with(value, vt, inner, is_sub);
    }
    // Array literal → array type. Lets `let a: i32[] = [1, 2, 3]` work
    // even though the literal's natural element type is i64, and lets
    // `let a: i32[3] = [1, 2, 3]` match a fixed-length annotation.
    if let (
        ExprKind::Array(elements),
        Type::Array {
            elem: target_elem,
            fixed: target_fixed,
        },
    ) = (&value.kind, target)
    {
        if let Some(n) = target_fixed {
            if elements.len() != *n {
                return false;
            }
        }
        // Empty literal: element type doesn't matter, accept whatever the
        // annotation asks for (subject to the length check above).
        if elements.is_empty() {
            return true;
        }
        let vt_elem = match vt {
            Type::Array { elem, .. } => elem.clone(),
            _ => return false,
        };
        return elements
            .iter()
            .all(|e| literal_assignable_with(e, &vt_elem, target_elem, is_sub));
    }
    // Array literal → SIMD type. Same idea as array → array but
    // length must match exactly and each element must fit the
    // lane scalar type (`[1.0, 2.0, 3.0, 4.0]` → `simd.f32x4`,
    // `[1, 2, 3, 4]` → `simd.i32x4` via int-narrowing).
    if let (ExprKind::Array(elements), Type::Simd { elem, lanes }) =
        (&value.kind, target)
    {
        if elements.len() != *lanes as usize {
            return false;
        }
        let lane_ty = elem.as_scalar_type();
        // Elements may carry any natural type (i64 from `1`, f64
        // from `2.0`); each one is literal-assignable to the lane
        // when its own bit-width / sign convention fits.
        let dummy_vt = lane_ty.clone();
        return elements
            .iter()
            .all(|e| literal_assignable_with(e, &dummy_vt, &lane_ty, is_sub));
    }
    // (Map literal subtyping is intentionally not handled here:
    // the JIT lays out Map<K, V> per (K, V) pair via interned
    // `MapKind`s and has no coerce for `Map<K, B>` → `Map<K, A>`,
    // so accepting it at TC time would diverge interpreter and
    // JIT. Annotate the entries explicitly or use `m.set(k, v)`
    // against a pre-typed `new Map<K, Parent>()` to upcast.)
    // Tuple literal → tuple type. Pairwise check so element-level
    // literal coercions (int width narrowing, fixed → dynamic array)
    // apply inside tuples just like they do at the top level.
    if let (ExprKind::Tuple(elements), Type::Tuple(target_elems)) = (&value.kind, target) {
        if elements.len() != target_elems.len() {
            return false;
        }
        let vt_elems = match vt {
            Type::Tuple(es) => es,
            _ => return false,
        };
        return elements
            .iter()
            .zip(vt_elems.iter())
            .zip(target_elems.iter())
            .all(|((e, vt_e), tt_e)| literal_assignable_with(e, vt_e, tt_e, is_sub));
    }
    // Int / unary-neg-int / float literal cases are handled at
    // the top of this function so they take precedence over the
    // general `assignable` int-narrowing rule.
    if let ExprKind::Float(_) = &value.kind {
        if target.is_float() {
            return true;
        }
    }
    false
}

#[derive(Debug, Clone)]
pub(super) struct Signature {
    pub(super) params: Vec<Type>,
    pub(super) ret: Type,
    /// `true` for built-ins like `console.log` that accept any number of
    /// arguments (each typed as `Any`). User-defined variadics are not
    /// yet supported (parser doesn't accept `...args`).
    pub(super) variadic: bool,
    /// Generic type parameters declared on the fn (e.g. `<T, U>`).
    /// Empty for non-generic fns. `params` / `ret` may reference these
    /// as `Type::TypeVar(name)`; concrete types are inferred from the
    /// arg expression types at each call site.
    pub(super) type_params: Vec<Symbol>,
    /// Span of the original `FnDecl` this signature came from. Used by
    /// the post-typecheck mangler to find the right declaration when
    /// rewriting overloaded fn names. `Span::dummy()` for built-ins.
    #[allow(dead_code)]
    pub(super) decl_span: Span,
    /// Default-value expressions for each parameter (`None` when the
    /// parameter has no default). Used at call sites to fill in
    /// missing trailing arguments. Always empty for built-ins and for
    /// the indirect-call path (no FnDecl behind it).
    pub(super) defaults: Vec<Option<Expr>>,
    /// `pub` modifier on the original declaration. Built-ins are
    /// always public. Drives cross-module access enforcement: a fn
    /// (or class member) defined in module `M` is reachable from a
    /// different module only when `is_pub` is `true`.
    pub(super) is_pub: bool,
    /// `Some(reason)` when the original `FnDecl` carries a
    /// `@deprecated("reason")` attribute. The reason may be the
    /// empty string for a no-arg `@deprecated`. Surfaces as a
    /// non-fatal warning at every call site, accumulated into
    /// `TypeChecker::type_warnings`.
    pub(super) deprecated: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct ClassSig {
    /// Names of generic type parameters on the class. Empty for
    /// non-generic classes. Field/method types may reference these as
    /// `Type::TypeVar(name)`; instantiation substitutes them.
    pub(super) type_params: Vec<Symbol>,
    pub(super) fields: HashMap<Symbol, Type>,
    /// `pub` flag per field. Drives cross-module access checks at
    /// `obj.field` / `obj.field = …` sites — a non-pub field is
    /// reachable only from the class's own module.
    pub(super) field_pub: HashMap<Symbol, bool>,
    /// Methods grouped by source name, allowing overloads. Resolution
    /// at each MethodCall (and `new C(args)` for `init`) site picks
    /// the best match the same way top-level fn overloads do.
    pub(super) methods: HashMap<Symbol, Vec<Signature>>,
    /// `get` / `set` accessors. `obj.x` reads dispatch through the
    /// getter, `obj.x = v` writes through the setter (when present).
    pub(super) properties: HashMap<Symbol, PropertySig>,
    /// Interfaces this class declares it implements. Includes only
    /// the *direct* declarations — the `class_implements` helper
    /// walks the parent chain to compose the full set.
    pub(super) implements: Vec<Symbol>,
    /// `static` methods — Vec per name to support overloading the
    /// same way instance methods do. Resolved at `ClassName.method(args)`
    /// call sites.
    pub(super) static_methods: HashMap<Symbol, Vec<Signature>>,
    /// `static` fields — class-level mutable storage. Read/write
    /// dispatched at `ClassName.field` field expressions.
    pub(super) static_fields: HashMap<Symbol, Type>,
    /// Per-static-field `pub` flag (mirrors `field_pub` for instance
    /// fields).
    pub(super) static_field_pub: HashMap<Symbol, bool>,
    /// Subset of `static_fields` declared with `const` (immutable —
    /// reassignment is rejected at type-check time).
    pub(super) static_const_fields: HashSet<Symbol>,
    /// `extends Parent` — single-inheritance parent. None for root
    /// classes (or built-ins). Used by `is_subclass`, super
    /// resolution, and vtable layout.
    pub(super) parent: Option<Symbol>,
    /// Per-method vtable slot index. Inherited methods keep the
    /// parent's slot; overrides reuse the same slot; new methods
    /// added in this class get fresh slots after the parent's last
    /// slot. The JIT reads this to lay out vtables.
    pub(super) method_slots: HashMap<Symbol, usize>,
    /// Total number of vtable slots (= max slot index + 1, or 0).
    /// Equals parent's `vtable_len` plus this class's newly-added
    /// methods.
    pub(super) vtable_len: usize,
    /// `Some(libname)` for `@extern("lib") class Foo {}` — the type
    /// is an opaque handle whose values come from native extern fns.
    /// `new`, fields, methods are all rejected on these.
    pub(super) extern_lib: Option<Symbol>,
    /// `true` for `@extern(C) struct Foo { ... }`. Field-type validation
    /// (primitives + repr_c only) and embedded-struct layout depend
    /// on this flag.
    pub(super) is_repr_c: bool,
    /// `true` for `union Foo { ... }` (top-level or inside
    /// `@extern(C) { ... }`). Distinguishes the "exactly one field
    /// must be initialized" rule for union literals from the
    /// "every field must be initialized" rule for struct literals.
    pub(super) is_union: bool,
    /// `true` when the class ends in a C99 flexible array member
    /// (`T[]` last field). `new ClassName(n)` accepts a single i64
    /// arg (the trailing element count) for these.
    pub(super) has_fam: bool,
    /// Defining module — derived from the class's declaration name
    /// (`sdl.Window` ⇒ `"sdl"`, top-level entry items ⇒ `""`). Used
    /// to gate cross-module access on non-pub members.
    pub(super) module: String,
}

/// Type-checker view of an interface. Methods are stored in
/// declaration order (= the canonical interface-slot index used at
/// MIR / runtime dispatch sites). `is_pub` / `module` are kept for
/// the visibility check the loader will hook into later.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(super) struct InterfaceSig {
    pub(super) methods: Vec<InterfaceMethodSig>,
    pub(super) is_pub: bool,
    pub(super) module: String,
    /// `@com` interfaces use raw COM-vtable dispatch instead of
    /// the class registry. Methods occupy slots in declaration
    /// order, prefixed by the parent's slot list.
    pub(super) is_com: bool,
    /// `interface X : Parent` parent. Slot inheritance for the
    /// @com path is built by concatenating the parent's `methods`
    /// before this one's.
    pub(super) parent: Option<Symbol>,
}

#[derive(Debug, Clone)]
pub(super) struct InterfaceMethodSig {
    pub(super) name: Symbol,
    pub(super) params: Vec<Type>,
    pub(super) ret: Type,
    /// `@optional` on the interface declaration. Implementing
    /// classes don't have to provide a body for this method; the
    /// conformance check skips it.
    pub(super) is_optional: bool,
}

#[derive(Debug, Clone)]
pub(super) struct PropertySig {
    pub(super) ty: Type,
    pub(super) has_get: bool,
    pub(super) has_set: bool,
    pub(super) is_pub: bool,
    /// `true` for `pub static get name(): T` / `pub static set name(v: T)`.
    /// Read sites: `ClassName.name`. Write sites: `ClassName.name = v`.
    pub(super) is_static: bool,
}

/// Type-checker view of an enum. Variants preserve declaration order so
/// the JIT can use the same indices as ordinal tags.
#[derive(Debug, Clone)]
pub(super) struct EnumSig {
    /// Generic type parameters declared on the enum (mirrors
    /// `ClassSig.type_params`). Empty for non-generic enums.
    /// Variant payloads may reference these as `Type::TypeVar`.
    pub(super) type_params: Vec<Symbol>,
    pub(super) variants: Vec<EnumVariantSig>,
    /// `@flags` enum — supports `|` `&` `^` `~` and a `has` method.
    pub(super) flags: bool,
    /// `enum X: T` repr type. `None` defaults to `i64`. `Some(Type::Str)`
    /// flags a `: string`-repr enum (used by the `enum-as-string`
    /// cast path).
    pub(super) repr: Option<Type>,
}

#[derive(Debug, Clone)]
pub(super) struct EnumVariantSig {
    pub(super) name: Symbol,
    pub(super) payload: VariantPayloadSig,
}

#[derive(Debug, Clone)]
pub(super) enum VariantPayloadSig {
    Unit,
    Tuple(Vec<Type>),
    Struct(Vec<(Symbol, Type)>),
}

type Vars = HashMap<Symbol, Type>;

#[derive(Debug, Default)]
pub struct TypeChecker {
    /// Top-level functions, keyed by source name. A name maps to a
    /// non-empty vec because user code may define multiple
    /// overloads (`fn print(n: i64)` + `fn print(s: string)`). At each
    /// call site we pick the best match by arg-type scoring; if a name
    /// has just one entry we still go through the same path.
    pub(super) fns: HashMap<Symbol, Vec<Signature>>,
    pub(super) classes: HashMap<Symbol, ClassSig>,
    pub(super) interfaces: HashMap<Symbol, InterfaceSig>,
    pub(super) enums: HashMap<Symbol, EnumSig>,
    pub(super) vars: HashMap<Symbol, Type>,
    /// Inferred type-argument vector for each generic-fn call site,
    /// keyed by the call expression's span. Populated during checking;
    /// consumed by the JIT's monomorphization pass. Values may contain
    /// `Type::TypeVar` when the call sits inside another generic
    /// context — the monomorphizer substitutes those at expansion time.
    /// Wrapped in `RefCell` because `check_expr` takes `&self`.
    pub(super) fn_call_type_args: std::cell::RefCell<HashMap<Span, (Symbol, Vec<Type>)>>,
    /// Inferred type-arg vector for each generic-enum-ctor call site.
    /// Same shape as `fn_call_type_args`; consumed by the JIT's
    /// enum-monomorphization pass.
    pub(super) enum_ctor_type_args: std::cell::RefCell<HashMap<Span, (Symbol, Vec<Type>)>>,
    /// Per-call-site choice when the callee is overloaded:
    /// `(name, index_into_self.fns[name])`. Used by the post-typecheck
    /// mangler to rewrite `Call.callee` to the per-overload mangled
    /// name when the name has more than one overload.
    pub(super) fn_overload_pick: std::cell::RefCell<HashMap<Span, (Symbol, usize)>>,
    /// Per-call-site method overload pick. Same idea as
    /// `fn_overload_pick` but keyed for class methods. The triple is
    /// `(class_name, method_name, sig_idx)`. Includes both regular
    /// MethodCall sites and the `init` resolved at `new C(args)`.
    pub(super) method_overload_pick: std::cell::RefCell<HashMap<Span, (Symbol, Symbol, usize)>>,
    /// Per-MethodCall-span set marking `block.invoke(...)` calls
    /// whose receiver is `ObjCBlock<fn(...): id>` (i.e. returns
    /// an i64). The mangler rewrites the method symbol on these
    /// spans to a distinct internal name so MIR can dispatch to
    /// the obj-to-obj runtime invoker without re-running the
    /// type checker. Inserted by check_method_call's ObjCBlock
    /// branch when R is an i64-ish type.
    pub(super) objc_invoke_obj_to_obj_spans: std::cell::RefCell<HashSet<Span>>,
    /// Stack of currently-open loops, with the kind that controls
    /// whether `break v` is allowed and the accumulated break-value
    /// type so a `loop { ... break v }` expression can take the type of
    /// `v`. `LoopKind::Loop` collects break types; `LoopKind::Other`
    /// (while / for) rejects `break v` outright.
    pub(super) loop_stack: std::cell::RefCell<Vec<LoopFrame>>,
    /// `true` while validating types or bodies inside an
    /// `@extern(C) { ... }` block. Allows raw C pointer / `void` /
    /// `char` / `size_t` / `ssize_t` types to appear; outside the
    /// block these types are rejected.
    pub(super) in_extern_c: std::cell::RefCell<bool>,
    /// Type parameters in scope for the currently-checking fn (the
    /// fn's own `<T, U, ...>` plus any class type params when the
    /// fn is a method). Used by `validate_type` so a body-local
    /// annotation like `let y: T[] = [x]` recognises the enclosing
    /// fn's type params instead of treating them as unknown
    /// classes. Saved / restored across nested `check_fn` calls.
    pub(super) current_type_params: std::cell::RefCell<Vec<Symbol>>,
    /// Per-`loop` expression: the unified break-value type that the
    /// loop evaluates to. Unit means no `break v` was seen. Consumed
    /// by the JIT lowering so it can allocate the right Cranelift
    /// `Variable` for the loop result.
    pub(super) loop_break_type: std::cell::RefCell<HashMap<Span, Type>>,
    /// Per-`FnExpr` span: the list of (name, type) free variables
    /// the body captures from the enclosing scope. The JIT's hoist
    /// pass reads this to lay out closure environments. Order is
    /// stable (insertion order); the JIT uses it as the offset
    /// order in the closure struct.
    pub(super) fn_expr_captures: std::cell::RefCell<HashMap<Span, Vec<(Symbol, Type)>>>,
    /// Per-`FnExpr` span: the lexical class of the enclosing method
    /// when the closure body directly references `this`. The JIT's
    /// hoist pass uses this to add a synthetic `this` capture and
    /// rewrite `ExprKind::This` references in the wrapper body to
    /// `Var("this")` so the captured value flows in via the standard
    /// closure-env mechanism. Empty for closures defined outside a
    /// class method or that don't mention `this`.
    pub(super) fn_expr_this_class: std::cell::RefCell<HashMap<Span, Symbol>>,
    /// Used by the JIT's post-hoist re-typecheck: for each
    /// closure wrapper FnDecl, the body's "free vars" actually
    /// resolve to captured values. Pre-populating the body's
    /// scope with these makes the second-pass check pass without
    /// special-casing in the type checker proper.
    pub closure_wrapper_captures: HashMap<Symbol, Vec<(Symbol, Type)>>,
    /// Closure wrappers whose body was lifted from inside a class
    /// method. Records the lexical class so the wrapper body's
    /// `super.method(...)` calls find the parent class. Populated by
    /// the JIT pipeline alongside `closure_wrapper_captures`; empty
    /// for non-JIT callers.
    pub closure_wrapper_class: HashMap<Symbol, Symbol>,
    /// Per-call-site default-arg fills: the trailing default
    /// expressions (already type-checked) that the post-typecheck
    /// pass must append to the Call's `args`. Keyed by the call
    /// expression's span.
    pub(super) call_default_fills: std::cell::RefCell<HashMap<Span, Vec<Expr>>>,
    /// Module the currently-checked top-level item belongs to —
    /// derived from the item name's prefix (`sdl.Window` ⇒
    /// `"sdl"`, entry items ⇒ `""`). `obj.field` / method-call
    /// access sites compare this against the resolved class /
    /// fn's module to enforce the `module-private default + pub
    /// for cross-module exposure` rule. Saved / restored across
    /// nested item checks.
    pub(super) current_module: std::cell::RefCell<String>,
    /// JIT pipelines re-run `check` on a monomorphized /
    /// hoisted program whose synthetic items lose their `pub`
    /// flags. The original user source already passed the
    /// visibility checks; rerunning them on the rewritten form
    /// surfaces false positives. Setting this flag (`true`)
    /// makes `require_visible` a no-op while still checking
    /// every other invariant. The CLI's first typecheck leaves
    /// it at `false`.
    pub skip_visibility: bool,
    /// Names introduced by `const x = …` statements (one-time
    /// assigned). Reassignment is rejected by the `Assign`
    /// arm. Cleared at each fn-body boundary so the same name
    /// can be a `let` in one fn and a `const` in another.
    /// Limitation: nested-block shadowing of a const with a
    /// `let` of the same name still surfaces as a const-
    /// reassign error. In practice rare enough to leave for a
    /// later pass.
    pub(super) const_names: std::cell::RefCell<HashSet<Symbol>>,
    /// Top-level `const NAME = expr` (runtime form, demoted from
    /// `Item::Const` by the loader when the RHS isn't a
    /// compile-time constant). Persists across fn-body
    /// boundaries — assigning to a top-level const from anywhere
    /// is rejected.
    pub(super) top_level_consts: std::cell::RefCell<HashSet<Symbol>>,
    /// Non-fatal diagnostics surfaced during checking — currently
    /// just `@deprecated` call-site notices. CLI prints them to
    /// stderr after a successful `check`; LSP emits them as
    /// `DiagnosticSeverity::WARNING`. Accumulated via `warn(...)`,
    /// consumed via `warnings()`.
    pub(super) type_warnings: std::cell::RefCell<Vec<TypeWarning>>,
}

/// A non-fatal diagnostic — surfaced alongside (not instead of)
/// successful type checking. Today's only producer is the
/// `@deprecated` attribute on a class method.
#[derive(Debug, Clone)]
pub struct TypeWarning {
    pub message: String,
    pub span: Span,
}

/// Extract the module portion from a possibly-prefixed item name.
/// `"sdl.Window"` ⇒ `"sdl"`, `"sdl_audio.AudioDevice"` ⇒
/// `"sdl_audio"`, `"Foo"` ⇒ `""` (entry module). The loader merges
/// imported items under `module.<name>`, so the prefix is the only
/// post-load module signal we have.
pub(super) fn module_of_name(name: &str) -> &str {
    match name.rfind('.') {
        Some(i) => &name[..i],
        None => "",
    }
}

#[derive(Debug)]
pub(super) enum LoopFrame {
    /// `loop { ... }` — `break v` allowed; `Option<Type>` is the
    /// (unified) value type recorded so far, `None` until first break.
    Loop(Option<Type>),
    /// `while` / `for` — only bare `break` allowed.
    Other,
}

impl TypeChecker {
    pub fn new() -> Self {
        let mut tc = Self::default();
        tc.install_builtins();
        tc
    }

    /// Look up a top-level (module-scope) binding's resolved type.
    /// Used by the REPL to find the type of `let` bindings between
    /// chunks so they can be promoted to persistent host slots.
    pub fn lookup_global(&self, name: Symbol) -> Option<Type> {
        self.vars.get(&name).cloned()
    }

    /// Non-fatal diagnostics accumulated during the last `check`
    /// call (mainly `@deprecated` call-site notices). Empty when
    /// nothing was flagged.
    pub fn warnings(&self) -> Vec<TypeWarning> {
        self.type_warnings.borrow().clone()
    }

    pub(super) fn warn(&self, span: Span, message: String) {
        self.type_warnings.borrow_mut().push(TypeWarning { message, span });
    }

    /// Map of generic-fn call site → (callee name, inferred type args).
    /// Filled in during `check`; consumed by the JIT monomorphizer.
    pub fn fn_call_type_args(&self) -> HashMap<Span, (Symbol, Vec<Type>)> {
        self.fn_call_type_args.borrow().clone()
    }

    /// Map of generic-enum-ctor call site → (enum name, inferred type
    /// args). Same purpose as `fn_call_type_args` but for `Box.full(42)`
    /// style constructors.
    pub fn enum_ctor_type_args(&self) -> HashMap<Span, (Symbol, Vec<Type>)> {
        self.enum_ctor_type_args.borrow().clone()
    }

    /// Per-call-site overload pick: `(callee_name, sig_idx)`. Consumed
    /// by the post-typecheck `mangle_overloads` pass so it knows which
    /// of N same-name decls each call should resolve to.
    pub fn fn_overload_picks(&self) -> HashMap<Span, (Symbol, usize)> {
        self.fn_overload_pick.borrow().clone()
    }

    /// Per-call-site default-arg fills. Each entry is the (already
    /// type-checked) trailing default expressions to append to the
    /// Call's `args`. The post-typecheck mangler walks this to
    /// rewrite the AST so downstream passes see fully-positional
    /// calls.
    pub fn call_default_fills(&self) -> HashMap<Span, Vec<Expr>> {
        self.call_default_fills.borrow().clone()
    }

    /// Per-call-site method overload pick:
    /// `(class_name, method_name, sig_idx)`. Used by the mangler to
    /// rewrite `MethodCall.method` and `New.init_method`.
    pub fn method_overload_picks(&self) -> HashMap<Span, (Symbol, Symbol, usize)> {
        self.method_overload_pick.borrow().clone()
    }

    /// Set of `block.invoke(...)` call-site spans whose receiver is
    /// `ObjCBlock<fn(...): id>`. The mangler reads this to rewrite
    /// `.invoke` → `.__invokeIdToId` so MIR can dispatch to the
    /// obj-to-obj invoker; for void-returning blocks the method
    /// name is left as `invoke` and the obj / void_bytes / etc.
    /// invokers handle them.
    pub fn objc_invoke_obj_to_obj_spans(&self) -> HashSet<Span> {
        self.objc_invoke_obj_to_obj_spans.borrow().clone()
    }

    /// Map of `loop` expression span → the loop's result type (the
    /// unified `break v` value type, or `Unit` if no `break v`).
    /// Consumed by the JIT lowering.
    pub fn loop_break_types(&self) -> HashMap<Span, Type> {
        self.loop_break_type.borrow().clone()
    }

    /// Per-`FnExpr` span → captured (name, type) list. Empty list
    /// when the closure is purely top-level / no locals captured.
    pub fn fn_expr_captures(&self) -> HashMap<Span, Vec<(Symbol, Type)>> {
        self.fn_expr_captures.borrow().clone()
    }

    /// Per-`FnExpr` span → lexical class symbol when the closure
    /// body directly mentions `this`. Used by the JIT hoist pass to
    /// add a synthetic `this` capture and route `ExprKind::This` in
    /// the wrapper body through the standard closure-env load.
    pub fn fn_expr_this_class(&self) -> HashMap<Span, Symbol> {
        self.fn_expr_this_class.borrow().clone()
    }

    /// `(class, slot) -> method_name` for every class — used by the
    /// JIT to lay out per-class vtables. Empty for root classes
    /// without methods.
    pub fn class_method_slots(&self) -> HashMap<Symbol, HashMap<Symbol, usize>> {
        self.classes
            .iter()
            .map(|(n, sig)| (n.clone(), sig.method_slots.clone()))
            .collect()
    }

    /// `class -> vtable size` (max slot index + 1). Used by the JIT
    /// when allocating vtables.
    pub fn class_vtable_lens(&self) -> HashMap<Symbol, usize> {
        self.classes
            .iter()
            .map(|(n, sig)| (n.clone(), sig.vtable_len))
            .collect()
    }

    /// `class -> parent` (single-inheritance only). Empty for root
    /// classes. The JIT walks this for super-call resolution and
    /// vtable inheritance.
    pub fn class_parents(&self) -> HashMap<Symbol, Symbol> {
        self.classes
            .iter()
            .filter_map(|(n, sig)| sig.parent.map(|p| (*n, p)))
            .collect()
    }


}


// ─── overload resolution ──────────────────────────────────────────────



