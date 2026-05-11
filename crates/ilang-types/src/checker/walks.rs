//! AST walkers shared across the type checker:
//!
//! - `block_uses_this_directly` / `expr_uses_this_directly`:
//!   detect whether a body references `this` or `super.method(...)`
//!   so the JIT wrapper remembers its lexical class for receiver
//!   capture / parent-method resolution.
//! - `walk_children` / `walk_block_children`: a one-level child
//!   visitor over an `Expr`, used by `refine_returns` and
//!   `collect_in_expr`. Intentionally does NOT descend into
//!   `FnExpr` — that body has its own `return` target and its own
//!   `this` semantics.
//! - `refine_returns`: propagate the enclosing fn's declared return
//!   type into early-return enum-ctor sites.
//! - `collect_this_field_assignments` + helpers: record `this.f =
//!   v` assignments in an init block for init-coverage analysis.

use std::collections::HashSet;

use ilang_ast::{Expr, ExprKind, StmtKind, Symbol, Type};

use super::TypeChecker;

pub(super) fn block_uses_this_directly(b: &ilang_ast::Block) -> bool {
    for s in &b.stmts {
        match &s.kind {
            StmtKind::Let { value, .. }
            | StmtKind::LetTuple { value, .. }
            | StmtKind::LetStruct { value, .. } => {
                if expr_uses_this_directly(value) {
                    return true;
                }
            }
            StmtKind::Expr(e) => {
                if expr_uses_this_directly(e) {
                    return true;
                }
            }
        }
    }
    if let Some(t) = &b.tail {
        if expr_uses_this_directly(t) {
            return true;
        }
    }
    false
}

pub(super) fn expr_uses_this_directly(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::This => true,
        // `super.method(...)` needs the wrapper to remember its
        // lexical class for parent-method resolution, and it
        // implicitly threads `this` as the receiver — same wrapper
        // plumbing as a bare `this` reference.
        ExprKind::SuperCall { .. } => true,
        // Recurse INTO nested FnExpr too — see the comment on
        // `block_uses_this_directly`. The TC visits each FnExpr in
        // turn, so an inner `this` will independently mark its own
        // span; we additionally need outer FnExprs to see through
        // their nested closures.
        ExprKind::FnExpr { body, .. } => block_uses_this_directly(body),
        ExprKind::Some(x) | ExprKind::Unary { expr: x, .. } => {
            expr_uses_this_directly(x)
        }
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
            expr_uses_this_directly(lhs) || expr_uses_this_directly(rhs)
        }
        ExprKind::Cast { expr, .. }
        | ExprKind::TypeTest { expr, .. }
        | ExprKind::TypeDowncast { expr, .. } => expr_uses_this_directly(expr),
        ExprKind::Call { args, .. }
        | ExprKind::New { args, .. } => args.iter().any(expr_uses_this_directly),
        ExprKind::Field { obj, .. } => expr_uses_this_directly(obj),
        ExprKind::MethodCall { obj, args, .. } => {
            expr_uses_this_directly(obj) || args.iter().any(expr_uses_this_directly)
        }
        ExprKind::Block(b) => block_uses_this_directly(b),
        ExprKind::If { cond, then_branch, else_branch } => {
            expr_uses_this_directly(cond)
                || block_uses_this_directly(then_branch)
                || else_branch.as_deref().map_or(false, expr_uses_this_directly)
        }
        ExprKind::IfLet { expr, then_branch, else_branch, .. } => {
            expr_uses_this_directly(expr)
                || block_uses_this_directly(then_branch)
                || else_branch.as_deref().map_or(false, expr_uses_this_directly)
        }
        ExprKind::While { cond, body } => {
            expr_uses_this_directly(cond) || block_uses_this_directly(body)
        }
        ExprKind::Loop { body } => block_uses_this_directly(body),
        ExprKind::ForIn { iter, body, .. } => {
            expr_uses_this_directly(iter) || block_uses_this_directly(body)
        }
        ExprKind::Range { start, end, .. } => {
            start.as_deref().map_or(false, expr_uses_this_directly)
                || end.as_deref().map_or(false, expr_uses_this_directly)
        }
        ExprKind::Return(opt) | ExprKind::Break(opt) => {
            opt.as_deref().map_or(false, expr_uses_this_directly)
        }
        ExprKind::Assign { value, .. } => expr_uses_this_directly(value),
        ExprKind::AssignField { obj, value, .. } => {
            expr_uses_this_directly(obj) || expr_uses_this_directly(value)
        }
        ExprKind::AssignIndex { obj, index, value } => {
            expr_uses_this_directly(obj)
                || expr_uses_this_directly(index)
                || expr_uses_this_directly(value)
        }
        ExprKind::Array(items) | ExprKind::Tuple(items) => {
            items.iter().any(expr_uses_this_directly)
        }
        ExprKind::StructLit { fields, .. } => {
            fields.iter().any(|(_, v)| expr_uses_this_directly(v))
        }
        ExprKind::MapLit(entries) => entries
            .iter()
            .any(|(k, v)| expr_uses_this_directly(k) || expr_uses_this_directly(v)),
        ExprKind::Index { obj, index } => {
            expr_uses_this_directly(obj) || expr_uses_this_directly(index)
        }
        ExprKind::EnumCtor { args, .. } => match args {
            ilang_ast::CtorArgs::Unit => false,
            ilang_ast::CtorArgs::Tuple(es) => es.iter().any(expr_uses_this_directly),
            ilang_ast::CtorArgs::Struct(fs) => fs.iter().any(|(_, v)| expr_uses_this_directly(v)),
        },
        ExprKind::Match { scrutinee, arms } => {
            expr_uses_this_directly(scrutinee)
                || arms.iter().any(|a| expr_uses_this_directly(&a.body))
        }
        _ => false,
    }
}

/// Visit every direct child Expr of `e`. Used by `refine_returns` (to
/// propagate the enclosing fn's declared return type into early-return
/// enum-ctor sites) and by `collect_in_expr` (to record `this.f = v`
/// assignments for init-coverage analysis).
///
/// `FnExpr` is intentionally NOT recursed into: its body has its own
/// `return` target and its own `this` semantics (the closure may
/// never be called), so neither caller wants to treat the inner
/// expressions as belonging to the surrounding function.
pub(super) fn walk_children(e: &Expr, f: &mut dyn FnMut(&Expr)) {
    match &e.kind {
        ExprKind::Some(x) | ExprKind::Unary { expr: x, .. } => f(x),
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
            f(lhs);
            f(rhs);
        }
        ExprKind::Cast { expr, .. }
        | ExprKind::TypeTest { expr, .. }
        | ExprKind::TypeDowncast { expr, .. } => f(expr),
        ExprKind::Call { args, .. } => {
            for a in args {
                f(a);
            }
        }
        ExprKind::Field { obj, .. } => f(obj),
        ExprKind::MethodCall { obj, args, .. } => {
            f(obj);
            for a in args {
                f(a);
            }
        }
        ExprKind::SuperCall { args, .. } => {
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
        ExprKind::If { cond, then_branch, else_branch } => {
            f(cond);
            walk_block_children(then_branch, f);
            if let Some(e) = else_branch {
                f(e);
            }
        }
        ExprKind::IfLet { expr, then_branch, else_branch, .. } => {
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
        ExprKind::Return(Some(x)) => f(x),
        ExprKind::Assign { value, .. } => f(value),
        ExprKind::AssignField { obj, value, .. } => {
            f(obj);
            f(value);
        }
        ExprKind::AssignIndex { obj, index, value } => {
            f(obj);
            f(index);
            f(value);
        }
        ExprKind::StructLit { fields, .. } => {
            for (_, v) in fields.iter() {
                f(v);
            }
        }
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
        _ => {}
    }
}

pub(super) fn walk_block_children(b: &ilang_ast::Block, f: &mut dyn FnMut(&Expr)) {
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

pub(super) fn refine_returns(tc: &TypeChecker, e: &Expr, target: &Type) {
    if let ExprKind::Return(Some(inner)) = &e.kind {
        tc.refine_enum_ctor_args(inner, target);
    }
    walk_children(e, &mut |c| refine_returns(tc, c, target));
}

pub(super) fn collect_this_field_assignments(b: &ilang_ast::Block, out: &mut HashSet<Symbol>) {
    for s in &b.stmts {
        collect_in_stmt(s, out);
    }
    if let Some(t) = &b.tail {
        collect_in_expr(t, out);
    }
}

fn collect_in_stmt(s: &ilang_ast::Stmt, out: &mut HashSet<Symbol>) {
    match &s.kind {
        ilang_ast::StmtKind::Let { value, .. }
        | ilang_ast::StmtKind::LetTuple { value, .. }
        | ilang_ast::StmtKind::LetStruct { value, .. } => collect_in_expr(value, out),
        ilang_ast::StmtKind::Expr(e) => collect_in_expr(e, out),
    }
}

fn collect_in_expr(e: &ilang_ast::Expr, out: &mut HashSet<Symbol>) {
    use ilang_ast::ExprKind as K;
    if let K::AssignField { obj, field, .. } = &e.kind {
        if matches!(obj.kind, K::This) {
            out.insert(*field);
        }
    }
    walk_children(e, &mut |c| collect_in_expr(c, out));
}
