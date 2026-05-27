//! Shared types + tiny predicates used by every sibling pass —
//! `ObjcMethod` / `ObjcClass` are the parsed shape, `ObjcCtx` is
//! the per-block context handed to every builder, and `ImpEntry`
//! summarises what `build_register_class_fn` needs to know about a
//! bodied method so it can attach its IMP via `class_addMethod`.

use std::collections::HashSet;

use ilang_ast::{Attribute, Block, Param, Symbol, Type};

use super::selector::SelectorCache;

pub(super) struct ObjcMethod {
    pub(super) name: Symbol,
    pub(super) selector: String,
    pub(super) params: Box<[Param]>,
    pub(super) ret: Option<Type>,
    /// `Some(block)` when the user wrote `{ ... }` after the
    /// signature — the body becomes the ilang-side IMP for an
    /// `@objc class : Parent` subclass override. `None` for plain
    /// declarations that just bind an existing ObjC method (the
    /// (iii) wrapper does the dispatch).
    pub(super) body: Option<Block>,
    pub(super) span: ilang_ast::Span,
    pub(super) is_pub: bool,
    pub(super) is_static: bool,
    /// `true` when the user wrote `override` before the method
    /// name. Required by the type checker for any method whose
    /// name matches an inherited slot from the parent @objc
    /// class — without it, the "hides parent method" check
    /// fires. The bare keyword sits between `pub` (or `static`)
    /// and the method name, matching plain ilang class syntax.
    pub(super) is_override: bool,
    /// User-supplied attributes other than `@objc(...)` —
    /// currently just `@deprecated("reason")` for ObjC-side
    /// soft-removal markers. Propagated onto the synthesised
    /// dispatch wrapper so the type checker can warn at call
    /// sites.
    pub(super) extra_attrs: Vec<Attribute>,
    /// `Some(Getter)` / `Some(Setter)` when this method was
    /// declared as `pub get name(): T` / `pub set name(v: T)`.
    /// The desugar emits a `PropertyDecl` instead of a method
    /// FnDecl so call sites read / write via the bare property
    /// (`node.size` / `node.size = s`) and the parser-level
    /// `properties` machinery handles dispatch.
    pub(super) accessor: Option<AccessorKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AccessorKind {
    Getter,
    Setter,
}

pub(super) struct ObjcClass {
    pub(super) name: Symbol,
    pub(super) is_pub: bool,
    /// `Some(parent)` for ilang-defined subclasses (`@objc class Foo : NSObject`).
    /// `None` for plain bindings to existing ObjC classes.
    pub(super) parent: Option<Symbol>,
    /// Additional interface bases listed after the parent
    /// (`@objc class MyApp : NSObject, NSAppDel, NSWinDel { … }`).
    /// Propagated onto the desugared `ClassDecl`'s `interfaces`
    /// field so the type checker's conformance pass kicks in.
    pub(super) interfaces: Vec<Symbol>,
    pub(super) methods: Vec<ObjcMethod>,
    pub(super) span: ilang_ast::Span,
}

/// Per-block context shared by every builder. Carries the libobjc
/// helper-symbol names (so two `@extern(ObjC)` blocks in the same
/// file don't collide), the selector cache, and the set of @objc
/// class names whose params / returns get `.handle` marshalling.
pub(super) struct ObjcCtx<'a> {
    pub(super) tag: &'a str,
    pub(super) sel_struct: Symbol,
    pub(super) sel_register: Symbol,
    pub(super) sel_cache: &'a SelectorCache,
    pub(super) class_struct: Symbol,
    pub(super) get_class: Symbol,
    pub(super) object_struct: Symbol,
    pub(super) allocate_pair: Symbol,
    pub(super) register_pair: Symbol,
    pub(super) class_add_method: Symbol,
    pub(super) dlsym: Symbol,
    pub(super) retain: Symbol,
    pub(super) release: Symbol,
    pub(super) class_names: &'a HashSet<Symbol>,
}

/// Records what the `register()` method needs to know about each
/// bodied @objc method so it can attach the corresponding IMP via
/// `class_addMethod`. The IMP itself is generated as a separate
/// C-ABI function (see `build_imp_fn`) and resolved at runtime
/// through `dlsym`.
pub(super) struct ImpEntry {
    pub(super) selector: String,
    pub(super) encoding: String,
    pub(super) imp_symbol: Symbol,
}

/// Apple ARC's NS_RETURNS_RETAINED family rule. The selector's
/// first word (lowercase letters until an uppercase letter, `:`,
/// or end) names the family — `alloc`, `new`, `copy`,
/// `mutableCopy`, `init`, and `retain` return +1. Everything else
/// is autoreleased.
pub(super) fn returns_retained_selector(selector: &str) -> bool {
    for family in &["alloc", "new", "copy", "mutableCopy", "init", "retain"] {
        if let Some(rest) = selector.strip_prefix(family) {
            let first = rest.chars().next();
            match first {
                None | Some(':') => return true,
                Some(c) if !c.is_lowercase() => return true,
                _ => {}
            }
        }
    }
    false
}

pub(super) fn is_objc_class_ty(t: &Type, class_names: &HashSet<Symbol>) -> bool {
    match t {
        Type::Object(name) => class_names.contains(name),
        _ => false,
    }
}

/// `Some(Type::Simd { .. })` if `t` is `simd.fNxM[]` / `simd.fNxM[K]`,
/// i.e. an array of SIMD values. The @objc desugar swaps such params
/// to `*const simd.fNxM` on the alias and emits an
/// `arr as *const simd.fNxM` cast in the wrapper body so the data
/// pointer (not the header) is what reaches `objc_msgSend`.
pub(super) fn simd_array_elem(t: &Type) -> Option<Type> {
    match t {
        Type::Array { elem, .. } if matches!(**elem, Type::Simd { .. }) => {
            Some((**elem).clone())
        }
        _ => None,
    }
}

pub(super) fn ret_class_symbol(t: &Type) -> Symbol {
    match t {
        Type::Object(n) => *n,
        _ => unreachable!("caller checks is_objc_class_ty first"),
    }
}
