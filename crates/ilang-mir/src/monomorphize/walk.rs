//! Generic AST walkers used by the per-instantiation rewrite
//! passes. Two pairs:
//!
//! - `walk_expr_children` / `walk_block_children`: read-only visit
//!   of an Expr / Block's direct children.
//! - `map_expr_children` / `map_block_children`: rebuild an Expr's
//!   `ExprKind` / a Block by mapping each direct-child Expr through
//!   `f`. Lets `rewrite_calls_in_expr`'s catch-all arm avoid
//!   enumerating every variant by hand.

use ilang_ast::{Block, Expr, ExprKind, Stmt, StmtKind};

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
        ExprKind::FnExpr { .. } => {
            // Anonymous fns are hoisted out before this pass; nothing to do.
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
    }
}

pub(super) fn walk_block_children(b: &Block, f: &mut dyn FnMut(&Expr)) {
    for s in &b.stmts {
        match &s.kind {
            StmtKind::Let { value, .. }
            | StmtKind::LetTuple { value, .. }
            | StmtKind::LetStruct { value, .. } => f(value),
            StmtKind::Expr(e) => f(e),
        }
    }
    if let Some(t) = &b.tail {
        f(t);
    }
}

/// Map every direct child of `e` through `f` and rebuild the Expr's
/// kind. Used by rewrite_calls_in_expr's catch-all arm so we don't
/// have to enumerate every variant by hand.
pub(super) fn map_expr_children(e: &Expr, f: &mut dyn FnMut(&Expr) -> Expr) -> ExprKind {
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
            ty: ty.clone(),
        },
        ExprKind::TypeTest { expr, ty } => ExprKind::TypeTest {
            expr: Box::new(f(expr)),
            ty: ty.clone(),
        },
        ExprKind::TypeDowncast { expr, ty } => ExprKind::TypeDowncast {
            expr: Box::new(f(expr)),
            ty: ty.clone(),
        },
        ExprKind::FnExpr { params, ret, body } => ExprKind::FnExpr {
            params: params.clone(),
            ret: ret.clone(),
            body: map_block_children(body, f),
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
            type_args: type_args.clone(),
            args: args.iter().map(|a| f(a)).collect(), init_method: init_method.clone(),
        },
        ExprKind::Block(b) => ExprKind::Block(map_block_children(b, f)),
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => ExprKind::If {
            cond: Box::new(f(cond)),
            then_branch: map_block_children(then_branch, f),
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
            then_branch: map_block_children(then_branch, f),
            else_branch: else_branch.as_ref().map(|e| Box::new(f(e))),
        },
        ExprKind::While { cond, body } => ExprKind::While {
            cond: Box::new(f(cond)),
            body: map_block_children(body, f),
        },
        ExprKind::Loop { body } => ExprKind::Loop {
            body: map_block_children(body, f),
        },
        ExprKind::ForIn { var, iter, body } => ExprKind::ForIn {
            var: var.clone(),
            iter: Box::new(f(iter)),
            body: map_block_children(body, f),
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
    }
}

pub(super) fn map_block_children(b: &Block, f: &mut dyn FnMut(&Expr) -> Expr) -> Block {
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
                        ty: ty.clone(),
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
