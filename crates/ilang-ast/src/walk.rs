//! Shared `ExprKind` traversal helpers.
//!
//! Every pass that walks the AST tends to write the same exhaustive
//! `ExprKind` match where most arms only recurse into the variant's
//! `Expr` / `Block` children — pure mechanical traversal. This
//! module collects that traversal in one place so each pass only
//! writes the arms where it does something beyond walking children
//! (type validation, symbol rewrite, scope tracking, ...).
//!
//! Three shapes are needed:
//!
//! | helper                       | binding   | shape           | typical caller |
//! |------------------------------|-----------|-----------------|----------------|
//! | [`walk_expr_children_ref`]   | `&Expr`   | check, returns Result | normalize::validate |
//! | [`walk_expr_children_mut`]   | `&mut Expr` | mutate in place      | normalize::dealias  |
//! | [`fold_expr_default`]        | `ExprKind` by move | reconstruct  | normalize::rewrite  |
//!
//! All three are exhaustive over `ExprKind` so adding a new variant
//! produces compile errors in three places. Callers that handle a
//! variant specially short-circuit before reaching these helpers.
//!
//! Living in `ilang-ast` (instead of in any single consumer crate)
//! lets parser, cli, and lsp share the same skeleton.

use crate::{Block, CtorArgs, Expr, ExprKind, MatchArm, Param, StmtKind};

/// Recurse into every value-bearing `Expr` / `Block` child of `e`,
/// passing each to the appropriate callback. Types and bare symbols
/// are NOT walked — passes that care invoke their own
/// `check_type` / `dealias_sym` from the matching special arm.
pub fn walk_expr_children_ref<E>(
    e: &Expr,
    visit_child: &mut impl FnMut(&Expr) -> Result<(), E>,
    visit_block: &mut impl FnMut(&Block) -> Result<(), E>,
) -> Result<(), E> {
    match &e.kind {
        // Single child.
        ExprKind::Unary { expr, .. }
        | ExprKind::Some(expr)
        | ExprKind::Await(expr)
        | ExprKind::Cast { expr, .. }
        | ExprKind::TypeTest { expr, .. }
        | ExprKind::TypeDowncast { expr, .. }
        | ExprKind::Field { obj: expr, .. }
        | ExprKind::Assign { value: expr, .. } => visit_child(expr)?,
        // Optional child.
        ExprKind::Return(opt) | ExprKind::Break(opt) => {
            if let Some(x) = opt {
                visit_child(x)?;
            }
        }
        // Two children.
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
            visit_child(lhs)?;
            visit_child(rhs)?;
        }
        ExprKind::Index { obj, index } => {
            visit_child(obj)?;
            visit_child(index)?;
        }
        ExprKind::AssignField { obj, value, .. } => {
            visit_child(obj)?;
            visit_child(value)?;
        }
        ExprKind::AssignIndex { obj, index, value } => {
            visit_child(obj)?;
            visit_child(index)?;
            visit_child(value)?;
        }
        // Receiver + args.
        ExprKind::MethodCall { obj, args, .. } => {
            visit_child(obj)?;
            for a in args.iter() {
                visit_child(a)?;
            }
        }
        // Args-only call shapes.
        ExprKind::Call { args, .. }
        | ExprKind::SuperCall { args, .. }
        | ExprKind::New { args, .. } => {
            for a in args.iter() {
                visit_child(a)?;
            }
        }
        ExprKind::StructLit { fields, .. } => {
            for (_, x) in fields.iter() {
                visit_child(x)?;
            }
        }
        ExprKind::EnumCtor { args, .. } => match args {
            CtorArgs::Unit => {}
            CtorArgs::Tuple(es) => {
                for a in es.iter() {
                    visit_child(a)?;
                }
            }
            CtorArgs::Struct(fs) => {
                for (_, a) in fs.iter() {
                    visit_child(a)?;
                }
            }
        },
        // Sequences.
        ExprKind::Array(es) | ExprKind::Tuple(es) => {
            for x in es.iter() {
                visit_child(x)?;
            }
        }
        ExprKind::MapLit(entries) => {
            for (k, v) in entries.iter() {
                visit_child(k)?;
                visit_child(v)?;
            }
        }
        // Control flow (Expr + Block combinations).
        ExprKind::If { cond, then_branch, else_branch } => {
            visit_child(cond)?;
            visit_block(then_branch)?;
            if let Some(e2) = else_branch {
                visit_child(e2)?;
            }
        }
        ExprKind::IfLet { expr, then_branch, else_branch, .. } => {
            visit_child(expr)?;
            visit_block(then_branch)?;
            if let Some(e2) = else_branch {
                visit_child(e2)?;
            }
        }
        ExprKind::While { cond, body } => {
            visit_child(cond)?;
            visit_block(body)?;
        }
        ExprKind::Loop { body } => visit_block(body)?,
        ExprKind::ForIn { iter, body, .. } => {
            visit_child(iter)?;
            visit_block(body)?;
        }
        ExprKind::Match { scrutinee, arms } => {
            visit_child(scrutinee)?;
            for arm in arms.iter() {
                visit_child(&arm.body)?;
            }
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start {
                visit_child(s)?;
            }
            if let Some(e2) = end {
                visit_child(e2)?;
            }
        }
        ExprKind::Block(b) => visit_block(b)?,
        ExprKind::FnExpr { params, body, .. } => {
            // Param `ty` and `ret` are types, walked separately by
            // the caller's special arm; here only the value-bearing
            // parts (defaults + body) are descended into.
            for p in params.iter() {
                if let Some(d) = &p.default {
                    visit_child(d)?;
                }
            }
            visit_block(body)?;
        }
        // Leaves.
        ExprKind::Var(_)
        | ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::This
        | ExprKind::None
        | ExprKind::Continue
        | ExprKind::Closure { .. } => {}
    }
    Ok(())
}

/// Single-callback variant of [`walk_expr_children_ref`] that
/// treats `Block` boundaries as transparent: each statement's
/// value expression and the block's tail are passed to
/// `visit_child` directly. Useful for walks that don't need to
/// know about block scope (e.g. "find every FnExpr boundary"),
/// where having to thread a second closure for blocks would
/// trip the borrow checker when both callbacks need the same
/// `&mut` state.
///
/// Statement `ty` annotations on `Let` are NOT walked — same
/// rationale as [`walk_expr_children_ref`].
pub fn walk_expr_descendants_ref<E>(
    e: &Expr,
    visit_child: &mut impl FnMut(&Expr) -> Result<(), E>,
) -> Result<(), E> {
    match &e.kind {
        // Single child.
        ExprKind::Unary { expr, .. }
        | ExprKind::Some(expr)
        | ExprKind::Await(expr)
        | ExprKind::Cast { expr, .. }
        | ExprKind::TypeTest { expr, .. }
        | ExprKind::TypeDowncast { expr, .. }
        | ExprKind::Field { obj: expr, .. }
        | ExprKind::Assign { value: expr, .. } => visit_child(expr)?,
        ExprKind::Return(opt) | ExprKind::Break(opt) => {
            if let Some(x) = opt {
                visit_child(x)?;
            }
        }
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
            visit_child(lhs)?;
            visit_child(rhs)?;
        }
        ExprKind::Index { obj, index } => {
            visit_child(obj)?;
            visit_child(index)?;
        }
        ExprKind::AssignField { obj, value, .. } => {
            visit_child(obj)?;
            visit_child(value)?;
        }
        ExprKind::AssignIndex { obj, index, value } => {
            visit_child(obj)?;
            visit_child(index)?;
            visit_child(value)?;
        }
        ExprKind::MethodCall { obj, args, .. } => {
            visit_child(obj)?;
            for a in args.iter() {
                visit_child(a)?;
            }
        }
        ExprKind::Call { args, .. }
        | ExprKind::SuperCall { args, .. }
        | ExprKind::New { args, .. } => {
            for a in args.iter() {
                visit_child(a)?;
            }
        }
        ExprKind::StructLit { fields, .. } => {
            for (_, x) in fields.iter() {
                visit_child(x)?;
            }
        }
        ExprKind::EnumCtor { args, .. } => match args {
            CtorArgs::Unit => {}
            CtorArgs::Tuple(es) => {
                for a in es.iter() {
                    visit_child(a)?;
                }
            }
            CtorArgs::Struct(fs) => {
                for (_, a) in fs.iter() {
                    visit_child(a)?;
                }
            }
        },
        ExprKind::Array(es) | ExprKind::Tuple(es) => {
            for x in es.iter() {
                visit_child(x)?;
            }
        }
        ExprKind::MapLit(entries) => {
            for (k, v) in entries.iter() {
                visit_child(k)?;
                visit_child(v)?;
            }
        }
        ExprKind::If { cond, then_branch, else_branch } => {
            visit_child(cond)?;
            descend_block_ref(then_branch, visit_child)?;
            if let Some(e2) = else_branch {
                visit_child(e2)?;
            }
        }
        ExprKind::IfLet { expr, then_branch, else_branch, .. } => {
            visit_child(expr)?;
            descend_block_ref(then_branch, visit_child)?;
            if let Some(e2) = else_branch {
                visit_child(e2)?;
            }
        }
        ExprKind::While { cond, body } => {
            visit_child(cond)?;
            descend_block_ref(body, visit_child)?;
        }
        ExprKind::Loop { body } => descend_block_ref(body, visit_child)?,
        ExprKind::ForIn { iter, body, .. } => {
            visit_child(iter)?;
            descend_block_ref(body, visit_child)?;
        }
        ExprKind::Match { scrutinee, arms } => {
            visit_child(scrutinee)?;
            for arm in arms.iter() {
                visit_child(&arm.body)?;
            }
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start {
                visit_child(s)?;
            }
            if let Some(e2) = end {
                visit_child(e2)?;
            }
        }
        ExprKind::Block(b) => descend_block_ref(b, visit_child)?,
        ExprKind::FnExpr { params, body, .. } => {
            for p in params.iter() {
                if let Some(d) = &p.default {
                    visit_child(d)?;
                }
            }
            descend_block_ref(body, visit_child)?;
        }
        ExprKind::Var(_)
        | ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::This
        | ExprKind::None
        | ExprKind::Continue
        | ExprKind::Closure { .. } => {}
    }
    Ok(())
}

fn descend_block_ref<E>(
    b: &Block,
    visit_child: &mut impl FnMut(&Expr) -> Result<(), E>,
) -> Result<(), E> {
    for s in &b.stmts {
        let v = match &s.kind {
            StmtKind::Let { value, .. }
            | StmtKind::LetTuple { value, .. }
            | StmtKind::LetStruct { value, .. } => value,
            StmtKind::Expr(e) => e,
        };
        visit_child(v)?;
    }
    if let Some(t) = &b.tail {
        visit_child(t)?;
    }
    Ok(())
}

/// Mutable-reference twin of [`walk_expr_children_ref`]. The
/// callbacks mutate each child in place; the helper returns `()`.
pub fn walk_expr_children_mut(
    e: &mut Expr,
    visit_child: &mut impl FnMut(&mut Expr),
    visit_block: &mut impl FnMut(&mut Block),
) {
    match &mut e.kind {
        ExprKind::Unary { expr, .. }
        | ExprKind::Some(expr)
        | ExprKind::Await(expr)
        | ExprKind::Cast { expr, .. }
        | ExprKind::TypeTest { expr, .. }
        | ExprKind::TypeDowncast { expr, .. }
        | ExprKind::Field { obj: expr, .. }
        | ExprKind::Assign { value: expr, .. } => visit_child(expr),
        ExprKind::Return(opt) | ExprKind::Break(opt) => {
            if let Some(x) = opt {
                visit_child(x);
            }
        }
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
            visit_child(lhs);
            visit_child(rhs);
        }
        ExprKind::Index { obj, index } => {
            visit_child(obj);
            visit_child(index);
        }
        ExprKind::AssignField { obj, value, .. } => {
            visit_child(obj);
            visit_child(value);
        }
        ExprKind::AssignIndex { obj, index, value } => {
            visit_child(obj);
            visit_child(index);
            visit_child(value);
        }
        ExprKind::MethodCall { obj, args, .. } => {
            visit_child(obj);
            for a in args.iter_mut() {
                visit_child(a);
            }
        }
        ExprKind::Call { args, .. }
        | ExprKind::SuperCall { args, .. }
        | ExprKind::New { args, .. } => {
            for a in args.iter_mut() {
                visit_child(a);
            }
        }
        ExprKind::StructLit { fields, .. } => {
            for (_, x) in fields.iter_mut() {
                visit_child(x);
            }
        }
        ExprKind::EnumCtor { args, .. } => match args {
            CtorArgs::Unit => {}
            CtorArgs::Tuple(es) => {
                for a in es.iter_mut() {
                    visit_child(a);
                }
            }
            CtorArgs::Struct(fs) => {
                for (_, a) in fs.iter_mut() {
                    visit_child(a);
                }
            }
        },
        ExprKind::Array(es) | ExprKind::Tuple(es) => {
            for x in es.iter_mut() {
                visit_child(x);
            }
        }
        ExprKind::MapLit(entries) => {
            for (k, v) in entries.iter_mut() {
                visit_child(k);
                visit_child(v);
            }
        }
        ExprKind::If { cond, then_branch, else_branch } => {
            visit_child(cond);
            visit_block(then_branch);
            if let Some(e2) = else_branch {
                visit_child(e2);
            }
        }
        ExprKind::IfLet { expr, then_branch, else_branch, .. } => {
            visit_child(expr);
            visit_block(then_branch);
            if let Some(e2) = else_branch {
                visit_child(e2);
            }
        }
        ExprKind::While { cond, body } => {
            visit_child(cond);
            visit_block(body);
        }
        ExprKind::Loop { body } => visit_block(body),
        ExprKind::ForIn { iter, body, .. } => {
            visit_child(iter);
            visit_block(body);
        }
        ExprKind::Match { scrutinee, arms } => {
            visit_child(scrutinee);
            for arm in arms.iter_mut() {
                visit_child(&mut arm.body);
            }
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start {
                visit_child(s);
            }
            if let Some(e2) = end {
                visit_child(e2);
            }
        }
        ExprKind::Block(b) => visit_block(b),
        ExprKind::FnExpr { params, body, .. } => {
            for p in params.iter_mut() {
                if let Some(d) = &mut p.default {
                    visit_child(d);
                }
            }
            visit_block(body);
        }
        ExprKind::Var(_)
        | ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::This
        | ExprKind::None
        | ExprKind::Continue
        | ExprKind::Closure { .. } => {}
    }
}

/// Owning-reconstruction default for the rewrite pass. Takes
/// `ExprKind` by move and rebuilds it with each child replaced by
/// `fold_child(child)`. Callers that need extra work (scope
/// tracking, receiver resolution) handle the relevant variants
/// before invoking this for everything else.
///
/// Note: `FnExpr` here recurses into param defaults + body via
/// `fold_child` / `fold_block`. The rewrite caller's own FnExpr
/// handler bypasses this default because it has scope-tracking
/// requirements the generic default can't express.
pub fn fold_expr_default(
    kind: ExprKind,
    fold_child: &mut impl FnMut(Expr) -> Expr,
    fold_block: &mut impl FnMut(Block) -> Block,
) -> ExprKind {
    match kind {
        ExprKind::Unary { op, expr } => {
            ExprKind::Unary { op, expr: Box::new(fold_child(*expr)) }
        }
        ExprKind::Some(inner) => ExprKind::Some(Box::new(fold_child(*inner))),
        ExprKind::Await(inner) => ExprKind::Await(Box::new(fold_child(*inner))),
        ExprKind::Cast { expr, ty } => {
            ExprKind::Cast { expr: Box::new(fold_child(*expr)), ty }
        }
        ExprKind::TypeTest { expr, ty } => {
            ExprKind::TypeTest { expr: Box::new(fold_child(*expr)), ty }
        }
        ExprKind::TypeDowncast { expr, ty } => {
            ExprKind::TypeDowncast { expr: Box::new(fold_child(*expr)), ty }
        }
        ExprKind::Field { obj, name } => {
            ExprKind::Field { obj: Box::new(fold_child(*obj)), name }
        }
        ExprKind::Assign { target, value } => {
            ExprKind::Assign { target, value: Box::new(fold_child(*value)) }
        }
        ExprKind::Return(opt) => {
            ExprKind::Return(opt.map(|e| Box::new(fold_child(*e))))
        }
        ExprKind::Break(opt) => {
            ExprKind::Break(opt.map(|e| Box::new(fold_child(*e))))
        }
        ExprKind::Binary { op, lhs, rhs } => ExprKind::Binary {
            op,
            lhs: Box::new(fold_child(*lhs)),
            rhs: Box::new(fold_child(*rhs)),
        },
        ExprKind::Logical { op, lhs, rhs } => ExprKind::Logical {
            op,
            lhs: Box::new(fold_child(*lhs)),
            rhs: Box::new(fold_child(*rhs)),
        },
        ExprKind::Index { obj, index } => ExprKind::Index {
            obj: Box::new(fold_child(*obj)),
            index: Box::new(fold_child(*index)),
        },
        ExprKind::AssignField { obj, field, value, is_init } => ExprKind::AssignField {
            obj: Box::new(fold_child(*obj)),
            field,
            value: Box::new(fold_child(*value)),
            is_init,
        },
        ExprKind::AssignIndex { obj, index, value } => ExprKind::AssignIndex {
            obj: Box::new(fold_child(*obj)),
            index: Box::new(fold_child(*index)),
            value: Box::new(fold_child(*value)),
        },
        ExprKind::MethodCall { obj, method, args } => ExprKind::MethodCall {
            obj: Box::new(fold_child(*obj)),
            method,
            args: Vec::from(args).into_iter().map(&mut *fold_child).collect(),
        },
        ExprKind::Call { callee, args } => ExprKind::Call {
            callee,
            args: Vec::from(args).into_iter().map(&mut *fold_child).collect(),
        },
        ExprKind::SuperCall { method, args } => ExprKind::SuperCall {
            method,
            args: Vec::from(args).into_iter().map(&mut *fold_child).collect(),
        },
        ExprKind::New { class, type_args, args, init_method } => ExprKind::New {
            class,
            type_args,
            args: Vec::from(args).into_iter().map(&mut *fold_child).collect(),
            init_method,
        },
        ExprKind::StructLit { class, fields, field_name_spans } => ExprKind::StructLit {
            class,
            fields: fields.into_iter().map(|(n, e)| (n, fold_child(e))).collect(),
            field_name_spans,
        },
        ExprKind::EnumCtor { enum_name, variant, args } => ExprKind::EnumCtor {
            enum_name,
            variant,
            args: match args {
                CtorArgs::Unit => CtorArgs::Unit,
                CtorArgs::Tuple(es) => CtorArgs::Tuple(
                    Vec::from(es).into_iter().map(&mut *fold_child).collect(),
                ),
                CtorArgs::Struct(fs) => CtorArgs::Struct(
                    fs.into_iter().map(|(n, e)| (n, fold_child(e))).collect(),
                ),
            },
        },
        ExprKind::Array(items) => ExprKind::Array(
            Vec::from(items).into_iter().map(&mut *fold_child).collect(),
        ),
        ExprKind::Tuple(items) => ExprKind::Tuple(
            Vec::from(items).into_iter().map(&mut *fold_child).collect(),
        ),
        ExprKind::MapLit(entries) => ExprKind::MapLit(
            entries
                .into_iter()
                .map(|(k, v)| (fold_child(k), fold_child(v)))
                .collect(),
        ),
        ExprKind::If { cond, then_branch, else_branch } => ExprKind::If {
            cond: Box::new(fold_child(*cond)),
            then_branch: fold_block(then_branch),
            else_branch: else_branch.map(|e| Box::new(fold_child(*e))),
        },
        ExprKind::IfLet { name, expr, then_branch, else_branch } => ExprKind::IfLet {
            name,
            expr: Box::new(fold_child(*expr)),
            then_branch: fold_block(then_branch),
            else_branch: else_branch.map(|e| Box::new(fold_child(*e))),
        },
        ExprKind::While { cond, body } => ExprKind::While {
            cond: Box::new(fold_child(*cond)),
            body: fold_block(body),
        },
        ExprKind::Loop { body } => ExprKind::Loop { body: fold_block(body) },
        ExprKind::ForIn { var, iter, body } => ExprKind::ForIn {
            var,
            iter: Box::new(fold_child(*iter)),
            body: fold_block(body),
        },
        ExprKind::Match { scrutinee, arms } => ExprKind::Match {
            scrutinee: Box::new(fold_child(*scrutinee)),
            arms: arms
                .into_iter()
                .map(|arm: MatchArm| MatchArm {
                    pattern: arm.pattern,
                    body: fold_child(arm.body),
                    span: arm.span,
                })
                .collect(),
        },
        ExprKind::Range { start, end, inclusive } => ExprKind::Range {
            start: start.map(|s| Box::new(fold_child(*s))),
            end: end.map(|e| Box::new(fold_child(*e))),
            inclusive,
        },
        ExprKind::Block(b) => ExprKind::Block(fold_block(b)),
        ExprKind::FnExpr { params, ret, body } => ExprKind::FnExpr {
            params: params
                .into_iter()
                .map(|p| Param {
                    name: p.name,
                    ty: p.ty,
                    span: p.span,
                    default: p.default.map(|d| fold_child(d)),
                })
                .collect(),
            ret,
            body: fold_block(body),
        },
        leaf @ (ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::Var(_)
        | ExprKind::This
        | ExprKind::None
        | ExprKind::Continue
        | ExprKind::Closure { .. }) => leaf,
    }
}
