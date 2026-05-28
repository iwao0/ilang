//! Buffer walker — visits the parsed AST of one open document and
//! pushes hover / F12 / documentHighlight entries into `Walker::refs`.
//!
//! Submodules:
//! * [`items`] — `walk_fn` / `walk_class` / `walk_interface` plus the
//!   parser-synth filters.
//! * [`stmts`] — `walk_block` / `walk_stmt`, the let-binding scope
//!   pushers.
//! * [`exprs`] — `walk_expr` and its per-`ExprKind` sub-handlers.
//! * [`refs`] — type-name resolution and the lower-level `push_decl*`
//!   / `push_ref*` ref-entry builders.
//! * [`infer`] — `infer_expr` / `resolve_obj_class` plus the built-in
//!   primitive / `Map` / `Set` method-return tables.
#![allow(unused_imports)]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use ilang_ast::{
    Block, ClassDecl, EnumDecl, Expr, ExprKind, FieldDecl, FnDecl, GenericTy, InterfaceDecl, Item,
    Param, Pattern, PatternBindings, PatternKind, Program, Span, Stmt, StmtKind,
    Symbol as AstSymbol, Type, VariantPayload,
};
use ilang_parser::parse as parse_program;
use ilang_types::{check, TypeError};

use crate::*;

mod exprs;
mod infer;
mod items;
mod refs;
mod stmts;

pub(crate) use items::is_parser_synth_field;

/// True when `t` contains a `Type::TypeVar(_)` anywhere in its tree.
/// Used to skip the substitution dance for the common (concrete-return)
/// case where there's no type variable to fill in.
pub(crate) fn type_mentions_typevar(t: &Type) -> bool {
    use Type::*;
    match t {
        TypeVar(_) => true,
        Array { elem, .. } => type_mentions_typevar(elem),
        Optional(inner) | Weak(inner) => type_mentions_typevar(inner),
        RawPtr { inner, .. } => type_mentions_typevar(inner),
        Tuple(es) => es.iter().any(type_mentions_typevar),
        Generic(g) => g.args.iter().any(type_mentions_typevar),
        Fn(ft) => type_mentions_typevar(&ft.ret) || ft.params.iter().any(type_mentions_typevar),
        _ => false,
    }
}

/// Walk `param` and `arg` in parallel; whenever `param` reaches a
/// `TypeVar(name)`, record `name -> arg` in `subst` (first wins so a
/// later mismatch can't overwrite an earlier successful binding).
/// Mismatched shapes silently no-op — we're best-effort for hover
/// rendering, not a full unifier. The walker calls this once per
/// argument; the accumulated `subst` is then applied to the function's
/// return type.
pub(crate) fn unify_typevars(
    param: &Type,
    arg: &Type,
    subst: &mut HashMap<AstSymbol, Type>,
) {
    use Type::*;
    match (param, arg) {
        (TypeVar(name), other) => {
            subst.entry(*name).or_insert(other.clone());
        }
        (Array { elem: pe, .. }, Array { elem: ae, .. }) => unify_typevars(pe, ae, subst),
        (Optional(pi), Optional(ai)) | (Weak(pi), Weak(ai)) => unify_typevars(pi, ai, subst),
        (RawPtr { inner: pi, .. }, RawPtr { inner: ai, .. }) => unify_typevars(pi, ai, subst),
        (Tuple(pes), Tuple(aes)) if pes.len() == aes.len() => {
            for (p, a) in pes.iter().zip(aes.iter()) {
                unify_typevars(p, a, subst);
            }
        }
        (Generic(pg), Generic(ag)) if pg.args.len() == ag.args.len() => {
            for (p, a) in pg.args.iter().zip(ag.args.iter()) {
                unify_typevars(p, a, subst);
            }
        }
        _ => {}
    }
}

/// Apply a TypeVar substitution to `t`, returning a fresh `Type` with
/// every `TypeVar(name)` replaced by `subst[name]` (if known). Unmapped
/// type vars are left as-is — hover then falls back to showing `T`
/// rather than throwing the result away entirely.
pub(crate) fn substitute_typevars(t: &Type, subst: &HashMap<AstSymbol, Type>) -> Type {
    use Type::*;
    match t {
        TypeVar(name) => subst.get(name).cloned().unwrap_or_else(|| t.clone()),
        Array { elem, fixed } => Array {
            elem: Box::new(substitute_typevars(elem, subst)),
            fixed: *fixed,
        },
        Optional(inner) => Optional(Box::new(substitute_typevars(inner, subst))),
        Weak(inner) => Weak(Box::new(substitute_typevars(inner, subst))),
        RawPtr { is_const, inner } => RawPtr {
            is_const: *is_const,
            inner: Box::new(substitute_typevars(inner, subst)),
        },
        Tuple(es) => Tuple(
            es.iter()
                .map(|e| substitute_typevars(e, subst))
                .collect::<Vec<_>>()
                .into(),
        ),
        Generic(g) => Generic(Box::new(GenericTy {
            base: g.base,
            args: g
                .args
                .iter()
                .map(|a| substitute_typevars(a, subst))
                .collect::<Vec<_>>()
                .into(),
        })),
        Fn(ft) => {
            let new_params: Vec<Type> = ft
                .params
                .iter()
                .map(|p| substitute_typevars(p, subst))
                .collect();
            Fn(Box::new(ilang_ast::FnTy {
                params: new_params.into(),
                ret: substitute_typevars(&ft.ret, subst),
            }))
        }
        _ => t.clone(),
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Binding {
    pub(crate) name: String,
    pub(crate) span: Span,
    /// Statically-known type, if we can pin it down. Used both for hover
    /// signature and to resolve `local.field` accesses to the right class.
    pub(crate) ty: Option<Type>,
    /// What kind of binder introduced this (let / param / for-in / match
    /// pattern). Carried into hover signatures so use sites read like
    /// the declaration.
    pub(crate) kind: BindKind,
    /// When `Some`, replaces the kind/ty-derived hover signature.
    /// Used for `let func = fn(name: T): R { ... }` where we want to
    /// show parameter names that `Type::Fn` itself doesn't carry.
    pub(crate) override_signature: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum BindKind {
    Let,
    Param,
    ForIn,
    Pattern,
}

impl BindKind {
    pub(crate) fn render(self, name: &str, ty: Option<&Type>) -> String {
        let prefix = match self {
            BindKind::Let => "let ",
            BindKind::Param => "(parameter) ",
            BindKind::ForIn => "(for-binding) ",
            BindKind::Pattern => "(pattern) ",
        };
        match ty {
            Some(t) => format!("{prefix}{name}: {t}"),
            None => format!("{prefix}{name}"),
        }
    }
}

pub(crate) struct Walker<'a> {
    pub(crate) text: &'a str,
    pub(crate) symbols: &'a HashMap<AstSymbol, Symbol>,
    pub(crate) classes: &'a HashMap<AstSymbol, ClassInfo>,
    /// Top-level fn return types, keyed by name. Used to infer
    /// `let x = call()` bindings.
    pub(crate) fn_returns: &'a HashMap<AstSymbol, Type>,
    /// Hover signatures for `module.name` references that the loader
    /// brought in from a `use module` statement.
    pub(crate) external_signatures: &'a HashMap<AstSymbol, String>,
    /// Doc comments for external (imported) decls, keyed the same as
    /// `external_signatures`.
    pub(crate) external_docs: &'a HashMap<AstSymbol, String>,
    /// Return types for the same set of external fns. Used when
    /// inferring `let x = math.sqrt(...)` etc.
    pub(crate) external_returns: &'a HashMap<AstSymbol, Type>,
    /// Parameter types for imported generic fns whose return mentions
    /// a type parameter (`arrayFromCArray<T>(p: *const T, …): T[]`).
    /// Drives call-site type-var substitution so hover shows the
    /// instantiated return (`u16[]`) instead of the raw `T[]`.
    pub(crate) external_fn_params: &'a HashMap<AstSymbol, Vec<Type>>,
    /// Source-file path for each `module.<decl>` so cross-file F12
    /// can navigate into the originating module.
    pub(crate) external_sources: &'a ExternalSources,
    pub(crate) refs: &'a mut Vec<RefEntry>,
    /// Variable-name → class-name index, populated whenever a binding's
    /// statically-known type resolves to a class. Drives completion on
    /// `obj.` for ordinary instance variables.
    pub(crate) var_classes: &'a mut HashMap<AstSymbol, String>,
    /// Variable-name → full type, used for completion on built-in
    /// receivers (`string`, `T[]`) where there's no class entry.
    pub(crate) var_types: &'a mut HashMap<AstSymbol, Type>,
    /// Buffer-local `const NAME: T = …` types. The loader inlines
    /// const references away in merged programs, but the buffer-side
    /// walker still sees them as `Var(NAME)` and needs a way to
    /// recover the const's static type for `let x = NAME`-style
    /// bindings.
    pub(crate) consts: &'a HashMap<AstSymbol, Type>,
    /// Class name of the method body currently being walked, if any.
    /// `walk_fn` saves the previous value before entering a method
    /// body and restores it on return. `infer_expr` reads this when
    /// resolving `Field { obj: This, name }` so hover on
    /// `this.field.field2` chains can find the enclosing class
    /// without `walk_fn`-style threading of `this_class` through
    /// every recursive inference call.
    pub(crate) current_this_class: Option<String>,
}
