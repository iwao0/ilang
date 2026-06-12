//! Generic AST walkers used by the per-instantiation rewrite
//! passes. Three pairs:
//!
//! - `walk_expr_children` / `walk_block_children`: read-only visit
//!   of an Expr / Block's direct children.
//! - `walk_types_in_expr` / `walk_types_in_block`: visit every
//!   `Type` annotation that appears directly inside an Expr's
//!   `ExprKind` or a Block's `Let` stmts. Does NOT recurse into
//!   child Exprs — pair with `walk_expr_children` for full coverage.
//! - `map_expr_children` / `map_block_children`: rebuild an Expr's
//!   `ExprKind` / a Block by mapping each direct-child Expr through
//!   `f`. Lets `rewrite_calls_in_expr`'s catch-all arm avoid
//!   enumerating every variant by hand.

use ilang_ast::{
    Block, ClassDecl, Expr, ExprKind, FieldDecl, FnDecl, Param, PropertyDecl, Stmt, StmtKind, Type,
};

pub(super) fn walk_expr_children(e: &Expr, f: &mut dyn FnMut(&Expr)) {
    match &e.kind {
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::Var(_)
        | ExprKind::This
        | ExprKind::None
        | ExprKind::Continue => {}
        ExprKind::Break(opt) => {
            if let Some(x) = opt {
                f(x);
            }
        }
        ExprKind::Some(x) | ExprKind::Await(x) | ExprKind::Unary { expr: x, .. } => f(x),
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
            f(lhs);
            f(rhs);
        }
        ExprKind::Cast { expr, .. }
        | ExprKind::TypeTest { expr, .. }
        | ExprKind::TypeDowncast { expr, .. } => f(expr),
        ExprKind::FnExpr { params, body, .. } => {
            // Walk param defaults and the body so seed / scan passes
            // catch generic refs inside anonymous-fn captures
            // (`fn outer<T>() { let cb = fn() { use::<T>() }; }`).
            for p in params {
                if let Some(d) = &p.default {
                    f(d);
                }
            }
            walk_block_children(body, f);
        }
        ExprKind::Call { args, .. } => {
            for a in args {
                f(a);
            }
        }
        ExprKind::SuperCall { args, .. } => {
            for a in args {
                f(a);
            }
        }
        ExprKind::Closure { .. } => {}
        ExprKind::Field { obj, .. } => f(obj),
        ExprKind::MethodCall { obj, args, .. } => {
            f(obj);
            for a in args {
                f(a);
            }
        }
        ExprKind::New { args, .. } => {
            for a in args {
                f(a);
            }
        }
        ExprKind::Block(b) => walk_block_children(b, f),
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            f(cond);
            walk_block_children(then_branch, f);
            if let Some(e) = else_branch {
                f(e);
            }
        }
        ExprKind::IfLet {
            expr,
            then_branch,
            else_branch,
            ..
        } => {
            f(expr);
            walk_block_children(then_branch, f);
            if let Some(e) = else_branch {
                f(e);
            }
        }
        ExprKind::While { cond, body } => {
            f(cond);
            walk_block_children(body, f);
        }
        ExprKind::Loop { body } => walk_block_children(body, f),
        ExprKind::ForIn { iter, body, .. } => {
            f(iter);
            walk_block_children(body, f);
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start {
                f(s);
            }
            if let Some(e) = end {
                f(e);
            }
        }
        ExprKind::Return(opt) => {
            if let Some(e) = opt {
                f(e);
            }
        }
        ExprKind::Assign { value, .. } => f(value),
        ExprKind::AssignField { value, .. } => f(value),
        ExprKind::AssignIndex { value, .. } => f(value),
        ExprKind::Array(items) => {
            for i in items {
                f(i);
            }
        }
        ExprKind::Tuple(items) => {
            for i in items {
                f(i);
            }
        }
        ExprKind::StructLit { fields, .. } => {
            for (_, e) in fields {
                f(e);
            }
        }
        ExprKind::MapLit(entries) => {
            for (k, v) in entries {
                f(k);
                f(v);
            }
        }
        ExprKind::Index { obj, index } => {
            f(obj);
            f(index);
        }
        ExprKind::EnumCtor { args, .. } => match args {
            ilang_ast::CtorArgs::Unit => {}
            ilang_ast::CtorArgs::Tuple(es) => {
                for x in es {
                    f(x);
                }
            }
            ilang_ast::CtorArgs::Struct(fs) => {
                for (_, x) in fs {
                    f(x);
                }
            }
        },
        ExprKind::Match { scrutinee, arms } => {
            f(scrutinee);
            for arm in arms {
                f(&arm.body);
            }
        }
        ExprKind::Template { parts } => {
            for p in parts.iter() {
                if let ilang_ast::TemplatePart::Expr(e) = p {
                    f(e);
                }
            }
        }
    }
}

pub(super) fn walk_block_children(b: &Block, f: &mut dyn FnMut(&Expr)) {
    walk_top_stmts(&b.stmts, b.tail.as_deref(), f);
}

/// Same stmts+tail walk as `walk_block_children`, but takes the
/// pieces directly so it also works for `Program`'s top-level
/// `stmts` / `tail` slots (which don't form a `Block`).
pub(super) fn walk_top_stmts(
    stmts: &[Stmt],
    tail: Option<&Expr>,
    f: &mut dyn FnMut(&Expr),
) {
    for s in stmts {
        match &s.kind {
            StmtKind::Let { value, .. }
            | StmtKind::LetTuple { value, .. }
            | StmtKind::LetStruct { value, .. } => f(value),
            StmtKind::Expr(e) => f(e),
        }
    }
    if let Some(t) = tail {
        f(t);
    }
}

/// Visit every `Type` annotation that appears directly inside
/// `e`'s `ExprKind` — Cast / TypeTest / TypeDowncast target
/// types, FnExpr param types and return, and New's `type_args`.
/// Does NOT recurse into child Exprs (pair with
/// `walk_expr_children`).
pub(super) fn walk_types_in_expr(e: &Expr, f: &mut dyn FnMut(&Type)) {
    match &e.kind {
        ExprKind::Cast { ty, .. }
        | ExprKind::TypeTest { ty, .. }
        | ExprKind::TypeDowncast { ty, .. } => f(ty),
        ExprKind::FnExpr { params, ret, .. } => {
            for p in params {
                f(&p.ty);
            }
            if let Some(t) = ret {
                f(t);
            }
        }
        ExprKind::New { type_args, .. } => {
            for t in type_args {
                f(t);
            }
        }
        _ => {}
    }
}

/// Pre-order visit every `Type` node reachable from `t`, including
/// `t` itself. Recurses through the structural carriers (`Generic`
/// args, `Array` elem, `Optional`/`Weak` inner, `Fn` params + ret).
/// Leaves the decision of *what* to do at each node to `f`.
pub(super) fn walk_types_pre(t: &Type, f: &mut dyn FnMut(&Type)) {
    f(t);
    match t {
        Type::Generic(g) => {
            for a in &g.args {
                walk_types_pre(a, f);
            }
        }
        Type::Array { elem, .. } => walk_types_pre(elem, f),
        Type::Optional(inner) | Type::Weak(inner) => walk_types_pre(inner, f),
        // Recurse into tuple elements so a generic instantiation that
        // appears only inside a tuple type is still discovered for
        // monomorphization (mirrors the `subst_type` / `rewrite_type`
        // tuple arms).
        Type::Tuple(elems) => {
            for e in elems {
                walk_types_pre(e, f);
            }
        }
        Type::Fn(ft) => {
            for p in &ft.params {
                walk_types_pre(p, f);
            }
            walk_types_pre(&ft.ret, f);
        }
        _ => {}
    }
}

/// Visit every `Type` annotation on a `Let` stmt inside `b`.
/// Like `walk_types_in_expr`, does not recurse into Expr children.
pub(super) fn walk_types_in_block(b: &Block, f: &mut dyn FnMut(&Type)) {
    walk_types_in_top_stmts(&b.stmts, f);
}

/// Same `Let { ty }` walk as `walk_types_in_block`, but for
/// `Program`'s top-level `stmts` slice.
pub(super) fn walk_types_in_top_stmts(stmts: &[Stmt], f: &mut dyn FnMut(&Type)) {
    for s in stmts {
        if let StmtKind::Let { ty: Some(t), .. } = &s.kind {
            f(t);
        }
    }
}

/// Map every direct child of `e` through `f` and rebuild the Expr's
/// kind. Type annotations carried directly by `ExprKind` (`Cast.ty`,
/// `TypeTest.ty`, `TypeDowncast.ty`, `FnExpr.params[].ty`,
/// `FnExpr.ret`, `New.type_args`) pass through `map_type`. Callers
/// that don't need to rewrite types pass `&mut |t: &Type| t.clone()`.
pub(super) fn map_expr_children<FE, FT>(
    e: &Expr,
    f: &mut FE,
    map_type: &mut FT,
) -> ExprKind
where
    FE: FnMut(&Expr) -> Expr,
    FT: FnMut(&Type) -> Type,
{
    match &e.kind {
        ExprKind::Int(n) => ExprKind::Int(*n),
        ExprKind::Float(x) => ExprKind::Float(*x),
        ExprKind::Bool(b) => ExprKind::Bool(*b),
        ExprKind::Str(s) => ExprKind::Str(s.clone()),
        ExprKind::Var(n) => ExprKind::Var(n.clone()),
        ExprKind::This => ExprKind::This,
        ExprKind::None => ExprKind::None,
        ExprKind::Break(opt) => ExprKind::Break(opt.as_ref().map(|e| Box::new(f(e)))),
        ExprKind::Continue => ExprKind::Continue,
        ExprKind::Some(x) => ExprKind::Some(Box::new(f(x))),
        ExprKind::Await(x) => ExprKind::Await(Box::new(f(x))),
        ExprKind::Unary { op, expr } => ExprKind::Unary {
            op: *op,
            expr: Box::new(f(expr)),
        },
        ExprKind::Binary { op, lhs, rhs } => ExprKind::Binary {
            op: *op,
            lhs: Box::new(f(lhs)),
            rhs: Box::new(f(rhs)),
        },
        ExprKind::Logical { op, lhs, rhs } => ExprKind::Logical {
            op: *op,
            lhs: Box::new(f(lhs)),
            rhs: Box::new(f(rhs)),
        },
        ExprKind::Cast { expr, ty } => ExprKind::Cast {
            expr: Box::new(f(expr)),
            ty: map_type(ty),
        },
        ExprKind::TypeTest { expr, ty } => ExprKind::TypeTest {
            expr: Box::new(f(expr)),
            ty: map_type(ty),
        },
        ExprKind::TypeDowncast { expr, ty } => ExprKind::TypeDowncast {
            expr: Box::new(f(expr)),
            ty: map_type(ty),
        },
        ExprKind::FnExpr { params, ret, body } => ExprKind::FnExpr {
            params: params
                .iter()
                .map(|p| ilang_ast::Param {
                    name: p.name.clone(),
                    ty: map_type(&p.ty),
                    span: p.span,
                    default: p.default.as_ref().map(|d| f(d)),
                })
                .collect(),
            ret: ret.as_ref().map(|t| map_type(t)),
            body: map_block_children(body, f, map_type),
        },
        ExprKind::Call { callee, args } => ExprKind::Call {
            callee: callee.clone(),
            args: args.iter().map(|a| f(a)).collect(),
        },
        ExprKind::SuperCall { method, args } => ExprKind::SuperCall {
            method: method.clone(),
            args: args.iter().map(|a| f(a)).collect(),
        },
        ExprKind::Closure { fn_name, captures } => ExprKind::Closure {
            fn_name: fn_name.clone(),
            captures: captures.clone(),
        },
        ExprKind::Field { obj, name } => ExprKind::Field {
            obj: Box::new(f(obj)),
            name: name.clone(),
        },
        ExprKind::MethodCall { obj, method, args } => ExprKind::MethodCall {
            obj: Box::new(f(obj)),
            method: method.clone(),
            args: args.iter().map(|a| f(a)).collect(),
        },
        ExprKind::New { class, type_args, args, init_method } => ExprKind::New {
            class: class.clone(),
            type_args: type_args.iter().map(|t| map_type(t)).collect(),
            args: args.iter().map(|a| f(a)).collect(),
            init_method: init_method.clone(),
        },
        ExprKind::Block(b) => ExprKind::Block(map_block_children(b, f, map_type)),
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => ExprKind::If {
            cond: Box::new(f(cond)),
            then_branch: map_block_children(then_branch, f, map_type),
            else_branch: else_branch.as_ref().map(|e| Box::new(f(e))),
        },
        ExprKind::IfLet {
            name,
            expr,
            then_branch,
            else_branch,
        } => ExprKind::IfLet {
                name: name.clone(),
            expr: Box::new(f(expr)),
            then_branch: map_block_children(then_branch, f, map_type),
            else_branch: else_branch.as_ref().map(|e| Box::new(f(e))),
        },
        ExprKind::While { cond, body } => ExprKind::While {
            cond: Box::new(f(cond)),
            body: map_block_children(body, f, map_type),
        },
        ExprKind::Loop { body } => ExprKind::Loop {
            body: map_block_children(body, f, map_type),
        },
        ExprKind::ForIn { var, iter, body } => ExprKind::ForIn {
            var: var.clone(),
            iter: Box::new(f(iter)),
            body: map_block_children(body, f, map_type),
        },
        ExprKind::Range { start, end, inclusive } => ExprKind::Range {
            start: start.as_ref().map(|s| Box::new(f(s))),
            end: end.as_ref().map(|e| Box::new(f(e))),
            inclusive: *inclusive,
        },
        ExprKind::Return(opt) => ExprKind::Return(opt.as_ref().map(|e| Box::new(f(e)))),
        ExprKind::Assign { target, value } => ExprKind::Assign {
            target: target.clone(),
            value: Box::new(f(value)),
        },
        ExprKind::AssignField { obj, field, value, is_init } => ExprKind::AssignField {
            obj: obj.clone(),
            field: field.clone(),
            value: Box::new(f(value)), is_init: *is_init },
        ExprKind::AssignIndex { obj, index, value } => ExprKind::AssignIndex {
            obj: obj.clone(),
            index: index.clone(),
            value: Box::new(f(value)),
        },
        ExprKind::Array(items) => ExprKind::Array(items.iter().map(|e| f(e)).collect()),
        ExprKind::Tuple(items) => ExprKind::Tuple(items.iter().map(|e| f(e)).collect()),
        ExprKind::StructLit { class, fields, field_name_spans } => ExprKind::StructLit {
            class: class.clone(),
            fields: fields.iter().map(|(n, e)| (n.clone(), f(e))).collect(),
            field_name_spans: field_name_spans.clone(),
        },
        ExprKind::MapLit(entries) => ExprKind::MapLit(
            entries.iter().map(|(k, v)| (f(k), f(v))).collect(),
        ),
        ExprKind::Index { obj, index } => ExprKind::Index {
            obj: Box::new(f(obj)),
            index: Box::new(f(index)),
        },
        ExprKind::EnumCtor {
            enum_name,
            variant,
            args,
        } => ExprKind::EnumCtor {
            enum_name: enum_name.clone(),
            variant: variant.clone(),
            args: match args {
                ilang_ast::CtorArgs::Unit => ilang_ast::CtorArgs::Unit,
                ilang_ast::CtorArgs::Tuple(es) => {
                    ilang_ast::CtorArgs::Tuple(es.iter().map(|e| f(e)).collect())
                }
                ilang_ast::CtorArgs::Struct(fs) => ilang_ast::CtorArgs::Struct(
                    fs.iter().map(|(n, e)| (n.clone(), f(e))).collect(),
                ),
            },
        },
        ExprKind::Match { scrutinee, arms } => ExprKind::Match {
            scrutinee: Box::new(f(scrutinee)),
            arms: arms
                .iter()
                .map(|arm| ilang_ast::MatchArm {
                    pattern: arm.pattern.clone(),
                    body: f(&arm.body),
                    span: arm.span,
                })
                .collect(),
        },
        ExprKind::Template { parts } => ExprKind::Template {
            parts: parts
                .iter()
                .map(|p| match p {
                    ilang_ast::TemplatePart::Str(s) => ilang_ast::TemplatePart::Str(s.clone()),
                    ilang_ast::TemplatePart::Expr(e2) => {
                        ilang_ast::TemplatePart::Expr(f(e2))
                    }
                })
                .collect(),
        },
    }
}

/// Rebuild `b` by mapping every direct-child `Expr` through `f`
/// and every `Let { ty }` annotation through `map_type`. Callers
/// that don't rewrite types pass `&mut |t: &Type| t.clone()`.
pub(super) fn map_block_children<FE, FT>(
    b: &Block,
    f: &mut FE,
    map_type: &mut FT,
) -> Block
where
    FE: FnMut(&Expr) -> Expr,
    FT: FnMut(&Type) -> Type,
{
    Block {
        stmts: b
            .stmts
            .iter()
            .map(|s| {
                let kind = match &s.kind {
                    StmtKind::Let { name, ty, value, .. } => StmtKind::Let {
                        is_pub: false,
                        is_const: false,
                        name: name.clone(),
                        ty: ty.as_ref().map(|t| map_type(t)),
                        value: f(value),
                    },
                    StmtKind::LetTuple { elems, value } => StmtKind::LetTuple {
                        elems: elems.clone(),
                        value: f(value),
                    },
                    StmtKind::LetStruct { class, fields, value } => StmtKind::LetStruct {
                        class: class.clone(),
                        fields: fields.clone(),
                        value: f(value),
                    },
                    StmtKind::Expr(e) => StmtKind::Expr(f(e)),
                };
                Stmt { kind, span: s.span, source_module: s.source_module.clone() }
            })
            .collect(),
        tail: b.tail.as_ref().map(|e| Box::new(f(e))),
    }
}

/// Rebuild a `FnDecl` by remapping every component through the
/// supplied closures: param `default` exprs go through
/// `map_expr`, the body `Block` through `map_block`, every
/// `Type` annotation (params + return) through `map_type`.
///
/// `is_pub` is fixed to `false` and `is_async` to `false`: the
/// type checker upstream already enforces visibility, and async
/// fns desugar before monomorphize. `intrinsic_name` passes
/// through — top-level fn `@intrinsic("...")` bindings need it
/// preserved so the lower pass can route to the runtime.
pub(super) fn map_fn_decl<FE, FB, FT>(
    f: &FnDecl,
    map_expr: &mut FE,
    map_block: &mut FB,
    map_type: &mut FT,
) -> FnDecl
where
    FE: FnMut(&Expr) -> Expr,
    FB: FnMut(&Block) -> Block,
    FT: FnMut(&Type) -> Type,
{
    FnDecl {
        is_pub: false,
        attrs: f.attrs.clone(),
        name: f.name.clone(),
        type_params: f.type_params.clone(),
        params: f
            .params
            .iter()
            .map(|p| Param {
                name: p.name.clone(),
                ty: map_type(&p.ty),
                span: p.span,
                default: p.default.as_ref().map(|d| map_expr(d)),
            })
            .collect(),
        ret: f.ret.as_ref().map(|t| map_type(t)),
        body: map_block(&f.body),
        span: f.span,
        is_override: f.is_override,
        is_async: false,
        intrinsic_name: f.intrinsic_name,
    }
}

/// Rebuild a `PropertyDecl` by mapping its declared `Type` and
/// every getter / setter body through `map_fn_decl`. Same
/// visibility / async conventions as `map_fn_decl`.
pub(super) fn map_property_decl<FE, FB, FT>(
    p: &PropertyDecl,
    map_expr: &mut FE,
    map_block: &mut FB,
    map_type: &mut FT,
) -> PropertyDecl
where
    FE: FnMut(&Expr) -> Expr,
    FB: FnMut(&Block) -> Block,
    FT: FnMut(&Type) -> Type,
{
    PropertyDecl {
        is_static: p.is_static,
        is_pub: false,
        name: p.name.clone(),
        ty: map_type(&p.ty),
        getter: p
            .getter
            .as_ref()
            .map(|g| map_fn_decl(g, map_expr, map_block, map_type)),
        setter: p
            .setter
            .as_ref()
            .map(|s| map_fn_decl(s, map_expr, map_block, map_type)),
        span: p.span,
    }
}

/// Rebuild a `ClassDecl` by mapping every `Type` annotation
/// (field / param / return / property type) and every method /
/// accessor body through the supplied mappers. Class-level
/// modifiers (`is_repr_c`, `is_handle`, `is_union`, `parent`,
/// `interfaces`, `type_params`, `attrs`, `static_fields`) pass
/// through clone. Same `is_pub: false` / `is_async: false`
/// convention as `map_fn_decl`.
pub(super) fn map_class_decl<FE, FB, FT>(
    c: &ClassDecl,
    map_expr: &mut FE,
    map_block: &mut FB,
    map_type: &mut FT,
) -> ClassDecl
where
    FE: FnMut(&Expr) -> Expr,
    FB: FnMut(&Block) -> Block,
    FT: FnMut(&Type) -> Type,
{
    ClassDecl {
        is_pub: false,
        extern_lib: c.extern_lib.clone(),
        is_repr_c: c.is_repr_c,
        is_packed: c.is_packed,
        is_handle: c.is_handle,
        is_union: c.is_union,
        name: c.name.clone(),
        parent: c.parent.clone(),
        interfaces: c.interfaces.clone(),
        type_params: c.type_params.clone(),
        fields: c
            .fields
            .iter()
            .map(|f| FieldDecl {
                is_pub: false,
                name: f.name.clone(),
                ty: map_type(&f.ty),
                span: f.span,
                bits: f.bits,
            })
            .collect(),
        methods: c
            .methods
            .iter()
            .map(|m| map_fn_decl(m, map_expr, map_block, map_type))
            .collect(),
        static_methods: c
            .static_methods
            .iter()
            .map(|m| map_fn_decl(m, map_expr, map_block, map_type))
            .collect(),
        static_fields: c.static_fields.clone(),
        properties: c
            .properties
            .iter()
            .map(|p| map_property_decl(p, map_expr, map_block, map_type))
            .collect(),
        attrs: c.attrs.clone(),
        span: c.span,
    }
}
