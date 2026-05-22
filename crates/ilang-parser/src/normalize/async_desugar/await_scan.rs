//! Walk the AST to determine whether any `await` occurs inside an
//! `async fn` body. Used by [`super::lower_async_fn`] to decide
//! between the trivial `Promise.resolve(...)` wrap and the full
//! state-machine lowering.

use ilang_ast::{Block, Expr, ExprKind, Stmt, StmtKind};

/// Walk a block tree to determine whether any `await` occurs anywhere
/// inside.
pub(super) fn body_contains_await(b: &Block) -> bool {
    for s in &b.stmts {
        if stmt_contains_await(s) {
            return true;
        }
    }
    if let Some(t) = b.tail.as_deref() {
        if expr_contains_await(t) {
            return true;
        }
    }
    false
}

fn stmt_contains_await(s: &Stmt) -> bool {
    match &s.kind {
        StmtKind::Let { value, .. }
        | StmtKind::LetTuple { value, .. }
        | StmtKind::LetStruct { value, .. } => expr_contains_await(value),
        StmtKind::Expr(e) => expr_contains_await(e),
    }
}

fn expr_contains_await(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Await(_) => true,
        ExprKind::Block(b) => body_contains_await(b),
        ExprKind::If { cond, then_branch, else_branch } => {
            expr_contains_await(cond)
                || body_contains_await(then_branch)
                || else_branch.as_deref().is_some_and(expr_contains_await)
        }
        ExprKind::IfLet { expr, then_branch, else_branch, .. } => {
            expr_contains_await(expr)
                || body_contains_await(then_branch)
                || else_branch.as_deref().is_some_and(expr_contains_await)
        }
        ExprKind::While { cond, body } => expr_contains_await(cond) || body_contains_await(body),
        ExprKind::Loop { body } => body_contains_await(body),
        ExprKind::ForIn { iter, body, .. } => expr_contains_await(iter) || body_contains_await(body),
        ExprKind::Match { scrutinee, arms } => {
            expr_contains_await(scrutinee)
                || arms.iter().any(|a| expr_contains_await(&a.body))
        }
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
            expr_contains_await(lhs) || expr_contains_await(rhs)
        }
        ExprKind::Unary { expr, .. }
        | ExprKind::Cast { expr, .. }
        | ExprKind::TypeTest { expr, .. }
        | ExprKind::TypeDowncast { expr, .. } => expr_contains_await(expr),
        ExprKind::Some(e) => expr_contains_await(e),
        ExprKind::Return(opt) | ExprKind::Break(opt) => {
            opt.as_deref().is_some_and(expr_contains_await)
        }
        ExprKind::Assign { value, .. } => expr_contains_await(value),
        ExprKind::AssignField { obj, value, .. } => {
            expr_contains_await(obj) || expr_contains_await(value)
        }
        ExprKind::AssignIndex { obj, index, value } => {
            expr_contains_await(obj) || expr_contains_await(index) || expr_contains_await(value)
        }
        ExprKind::Call { args, .. }
        | ExprKind::SuperCall { args, .. }
        | ExprKind::New { args, .. } => args.iter().any(expr_contains_await),
        ExprKind::MethodCall { obj, args, .. } => {
            expr_contains_await(obj) || args.iter().any(expr_contains_await)
        }
        ExprKind::Field { obj, .. } => expr_contains_await(obj),
        ExprKind::Index { obj, index } => expr_contains_await(obj) || expr_contains_await(index),
        ExprKind::Tuple(es) | ExprKind::Array(es) => es.iter().any(expr_contains_await),
        ExprKind::Range { start, end, .. } => {
            start.as_deref().is_some_and(expr_contains_await)
                || end.as_deref().is_some_and(expr_contains_await)
        }
        ExprKind::FnExpr { .. } | ExprKind::Closure { .. } => false,
        _ => false,
    }
}
