//! Pattern-binding shape and type resolution.
//!
//! These helpers feed two parts of the state-machine builder:
//!
//! - `coerce_to_block` / `mid_body_join_kind` decide how `if` / `match`
//!   expressions translate into segments (tail vs mid-body join).
//! - `pattern_binding_types` resolves the precise type of each
//!   pattern-introduced binding (`some(v)`, `ok(v)`, `Box.hold(s)`,
//!   etc.) using the scrutinee's static type plus the program's
//!   enum table. `resolve_var_ty` and `substitute_type` are the
//!   small primitives it composes.

use std::collections::HashMap;

use ilang_ast::{
    Block, EnumDecl, Expr, ExprKind, Pattern, PatternBindings, PatternKind, Symbol, Type,
    VariantPayload,
};

use super::{block_has_await, expr_has_await};

/// Wrap an arbitrary `Expr` as a `Block` whose tail is that expr
/// (when the expr isn't already a `Block`). Used to normalize
/// `else if` chains and bare expression else / arm bodies into the
/// uniform Block shape that `build_block` walks.
pub(super) fn coerce_to_block(e: &Expr) -> Block {
    match &e.kind {
        ExprKind::Block(b) => b.clone(),
        _ => Block {
            stmts: Vec::new(),
            tail: Some(Box::new(e.clone())),
        },
    }
}

/// Returns Some(()) if `e` is a `let X = <if|match>` shape whose
/// branches / arms contain awaits — i.e. it needs the mid-body
/// join lowering rather than the simple sync-let path.
pub(super) fn mid_body_join_kind(e: &Expr) -> Option<()> {
    match &e.kind {
        ExprKind::If { cond, then_branch, else_branch } => {
            if expr_has_await(cond) {
                return None;
            }
            let has = block_has_await(then_branch)
                || else_branch.as_deref().is_some_and(expr_has_await);
            if has { Some(()) } else { None }
        }
        ExprKind::Match { scrutinee, arms } => {
            if expr_has_await(scrutinee) {
                return None;
            }
            let has = arms.iter().any(|a| expr_has_await(&a.body));
            if has { Some(()) } else { None }
        }
        _ => None,
    }
}

/// Try to resolve the static type of a Var expression by looking
/// it up in the cumulative live-set (which contains params + let
/// bindings introduced upstream). Returns None for non-Var or
/// unknown names — caller falls back to the I64 placeholder.
pub(super) fn resolve_var_ty(e: &Expr, cumulative_fields: &[(Symbol, Type)]) -> Option<Type> {
    if let ExprKind::Var(name) = &e.kind {
        for (n, t) in cumulative_fields {
            if n == name {
                return Some(t.clone());
            }
        }
    }
    None
}

/// Substitute every `Type::TypeVar(name)` in `t` with its mapped
/// concrete type from `subst`. Used when resolving variant payload
/// types of a generic enum instantiation (e.g. `Box<i64>` mapping
/// `T → i64` over a variant's `T` payload).
fn substitute_type(t: &Type, subst: &HashMap<Symbol, Type>) -> Type {
    match t {
        Type::TypeVar(n) => subst.get(n).cloned().unwrap_or_else(|| t.clone()),
        // The parser can't distinguish a type-parameter reference from
        // a class name at parse time — it emits `Type::Object(name)`
        // for both. If `name` is in the substitution map, treat it
        // as a type-param reference and replace.
        Type::Object(n) if subst.contains_key(n) => subst.get(n).cloned().unwrap(),
        Type::Optional(inner) => Type::Optional(Box::new(substitute_type(inner, subst))),
        Type::Weak(inner) => Type::Weak(Box::new(substitute_type(inner, subst))),
        Type::Array { elem, fixed } => Type::Array {
            elem: Box::new(substitute_type(elem, subst)),
            fixed: *fixed,
        },
        Type::Tuple(ts) => Type::Tuple(
            ts.iter().map(|t| substitute_type(t, subst)).collect::<Vec<_>>().into_boxed_slice(),
        ),
        Type::Generic(g) => Type::generic(
            g.base,
            g.args.iter().map(|a| substitute_type(a, subst)).collect(),
        ),
        Type::Fn(f) => Type::func(
            f.params.iter().map(|p| substitute_type(p, subst)).collect(),
            substitute_type(&f.ret, subst),
        ),
        _ => t.clone(),
    }
}

/// Compute (binding_name, binding_type) pairs introduced by `pattern`
/// when matched against a value of `scrutinee_ty`. Built-in
/// `Optional<T>` / `Result<T,E>` / user enums (including generic
/// ones via type_param substitution) are resolved precisely;
/// unknown shapes yield `Type::I64` placeholders for any bindings.
pub(super) fn pattern_binding_types(
    pattern: &Pattern,
    scrutinee_ty: Option<&Type>,
    enums: &HashMap<Symbol, EnumDecl>,
) -> Vec<(Symbol, Type)> {
    let PatternKind::Variant { variant, bindings, .. } = &pattern.kind else {
        return Vec::new();
    };
    let mut payload_tys: Option<Vec<Type>> = None;
    let mut payload_struct: HashMap<Symbol, Type> = HashMap::new();
    // Resolve user-enum payload using an enum decl + type-arg
    // substitution.
    let mut resolve_user_enum = |ed: &EnumDecl, subst: HashMap<Symbol, Type>| {
        if let Some(v) = ed.variants.iter().find(|v| v.name == *variant) {
            match &v.payload {
                VariantPayload::Unit => payload_tys = Some(Vec::new()),
                VariantPayload::Tuple(tys) => {
                    payload_tys =
                        Some(tys.iter().map(|t| substitute_type(t, &subst)).collect());
                }
                VariantPayload::Struct(fs) => {
                    for f in fs.iter() {
                        payload_struct.insert(f.name, substitute_type(&f.ty, &subst));
                    }
                }
            }
        }
    };
    match scrutinee_ty {
        Some(Type::Optional(inner)) => match variant.as_str() {
            "some" => payload_tys = Some(vec![(**inner).clone()]),
            "none" => payload_tys = Some(Vec::new()),
            _ => {}
        },
        Some(Type::Generic(g)) if g.base.as_str() == "Result" => {
            match (variant.as_str(), g.args.get(0), g.args.get(1)) {
                ("ok", Some(t), _) => payload_tys = Some(vec![t.clone()]),
                ("err", _, Some(e)) => payload_tys = Some(vec![e.clone()]),
                _ => {}
            }
        }
        Some(Type::Generic(g)) => {
            // User-defined generic enum: `EnumName<A, B>`. Build a
            // type_param → arg substitution and resolve.
            if let Some(ed) = enums.get(&g.base) {
                let subst: HashMap<Symbol, Type> = ed
                    .type_params
                    .iter()
                    .zip(g.args.iter())
                    .map(|(p, a)| (*p, a.clone()))
                    .collect();
                resolve_user_enum(ed, subst);
            }
        }
        Some(Type::Enum(name)) | Some(Type::Object(name)) => {
            if let Some(ed) = enums.get(name) {
                resolve_user_enum(ed, HashMap::new());
            }
        }
        _ => {}
    }
    match bindings {
        PatternBindings::Unit => Vec::new(),
        PatternBindings::Tuple(names) => names
            .iter()
            .enumerate()
            .filter(|(_, n)| n.as_str() != "_")
            .map(|(i, n)| {
                let t = payload_tys
                    .as_ref()
                    .and_then(|ts| ts.get(i).cloned())
                    .unwrap_or(Type::I64);
                (*n, t)
            })
            .collect(),
        PatternBindings::Struct(pairs) => pairs
            .iter()
            .filter(|(_, b)| b.as_str() != "_")
            .map(|(field, bind)| {
                let t = payload_struct.get(field).cloned().unwrap_or(Type::I64);
                (*bind, t)
            })
            .collect(),
    }
}
