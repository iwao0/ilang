//! Extracted from `checker/mod.rs`.

#![allow(unused_imports)]

use std::collections::{HashMap, HashSet};

use ilang_ast::{
    Block, ClassDecl, CtorArgs, EnumDecl, Expr, ExprKind, FieldDecl, FnDecl, Item, Param,
    PatternBindings, PatternKind, Program, Span, Stmt, StmtKind, Symbol, Type, UnOp,
    VariantPayload,
};

use crate::error::TypeError;
use crate::ops::{assignable, bin_result, int_literal_fits};

use super::*;

pub(super) fn collect_fn_expr_free_vars(
    b: &ilang_ast::Block,
    bound: &mut std::collections::HashSet<Symbol>,
    frees: &mut Vec<Symbol>,
    seen: &mut std::collections::HashSet<Symbol>,
) {
    let snapshot = bound.clone();
    for s in &b.stmts {
        match &s.kind {
            ilang_ast::StmtKind::Let { name, value, .. } => {
                cfev_expr(value, bound, frees, seen);
                bound.insert(name.clone());
            }
            ilang_ast::StmtKind::LetTuple { elems, value } => {
                cfev_expr(value, bound, frees, seen);
                for slot in elems.iter() {
                    if let Some(n) = slot {
                        bound.insert(n.clone());
                    }
                }
            }
            ilang_ast::StmtKind::LetStruct { fields, value, .. } => {
                cfev_expr(value, bound, frees, seen);
                for f in fields.iter() {
                    bound.insert(f.clone());
                }
            }
            ilang_ast::StmtKind::Expr(e) => cfev_expr(e, bound, frees, seen),
        }
    }
    if let Some(t) = &b.tail {
        cfev_expr(t, bound, frees, seen);
    }
    *bound = snapshot;
}

pub(super) fn cfev_expr(
    e: &ilang_ast::Expr,
    bound: &mut std::collections::HashSet<Symbol>,
    frees: &mut Vec<Symbol>,
    seen: &mut std::collections::HashSet<Symbol>,
) {
    use ilang_ast::ExprKind;
    match &e.kind {
        ExprKind::Var(n) => {
            if !bound.contains(n) && !seen.contains(n) {
                seen.insert(n.clone());
                frees.push(n.clone());
            }
        }
        ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_) | ExprKind::Str(_)
        | ExprKind::This | ExprKind::None | ExprKind::Continue => {}
        ExprKind::Break(opt) | ExprKind::Return(opt) => {
            if let Some(x) = opt { cfev_expr(x, bound, frees, seen); }
        }
        ExprKind::Some(inner) => cfev_expr(inner, bound, frees, seen),
        ExprKind::Await(inner) => cfev_expr(inner, bound, frees, seen),
        ExprKind::Unary { expr, .. } => cfev_expr(expr, bound, frees, seen),
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
            cfev_expr(lhs, bound, frees, seen);
            cfev_expr(rhs, bound, frees, seen);
        }
        ExprKind::Cast { expr, .. }
        | ExprKind::TypeTest { expr, .. }
        | ExprKind::TypeDowncast { expr, .. } => cfev_expr(expr, bound, frees, seen),
        ExprKind::Call { callee, args } => {
            // The callee may resolve to a fn-typed local / capture
            // rather than a top-level fn (`compose(f, g) { fn(x) {
            // f(g(x)) } }` calls captures `f`/`g`). Add the callee
            // name to the free set; the FnExpr capture-build step
            // filters out names not in the outer env, so top-level
            // fn references drop out automatically.
            if !bound.contains(callee) && seen.insert(callee.clone()) {
                frees.push(callee.clone());
            }
            for a in args { cfev_expr(a, bound, frees, seen); }
        }
        ExprKind::SuperCall { args, .. } => {
            for a in args { cfev_expr(a, bound, frees, seen); }
        }
        ExprKind::Field { obj, .. } => cfev_expr(obj, bound, frees, seen),
        ExprKind::MethodCall { obj, args, .. } => {
            cfev_expr(obj, bound, frees, seen);
            for a in args { cfev_expr(a, bound, frees, seen); }
        }
        ExprKind::New { args, .. } => {
            for a in args { cfev_expr(a, bound, frees, seen); }
        }
        ExprKind::Block(b) => collect_fn_expr_free_vars(b, bound, frees, seen),
        ExprKind::If { cond, then_branch, else_branch } => {
            cfev_expr(cond, bound, frees, seen);
            collect_fn_expr_free_vars(then_branch, bound, frees, seen);
            if let Some(x) = else_branch { cfev_expr(x, bound, frees, seen); }
        }
        ExprKind::IfLet { name, expr, then_branch, else_branch } => {
            cfev_expr(expr, bound, frees, seen);
            let snap = bound.clone();
            bound.insert(name.clone());
            collect_fn_expr_free_vars(then_branch, bound, frees, seen);
            *bound = snap;
            if let Some(x) = else_branch { cfev_expr(x, bound, frees, seen); }
        }
        ExprKind::While { cond, body } => {
            cfev_expr(cond, bound, frees, seen);
            collect_fn_expr_free_vars(body, bound, frees, seen);
        }
        ExprKind::Loop { body } => collect_fn_expr_free_vars(body, bound, frees, seen),
        ExprKind::ForIn { var, iter, body } => {
            cfev_expr(iter, bound, frees, seen);
            let snap = bound.clone();
            bound.insert(var.clone());
            collect_fn_expr_free_vars(body, bound, frees, seen);
            *bound = snap;
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start {
                cfev_expr(s, bound, frees, seen);
            }
            if let Some(e) = end {
                cfev_expr(e, bound, frees, seen);
            }
        }
        ExprKind::Assign { target, value } => {
            if !bound.contains(target) && !seen.contains(target) {
                seen.insert(target.clone());
                frees.push(target.clone());
            }
            cfev_expr(value, bound, frees, seen);
        }
        ExprKind::AssignField { obj, value, .. } => {
            cfev_expr(obj, bound, frees, seen);
            cfev_expr(value, bound, frees, seen);
        }
        ExprKind::AssignIndex { obj, index, value } => {
            cfev_expr(obj, bound, frees, seen);
            cfev_expr(index, bound, frees, seen);
            cfev_expr(value, bound, frees, seen);
        }
        ExprKind::Array(items) => for i in items { cfev_expr(i, bound, frees, seen); },
        ExprKind::Tuple(items) => for i in items { cfev_expr(i, bound, frees, seen); },
        ExprKind::StructLit { fields, .. } => {
            for (_, e) in fields { cfev_expr(e, bound, frees, seen); }
        }
        ExprKind::MapLit(entries) => for (k, v) in entries {
            cfev_expr(k, bound, frees, seen);
            cfev_expr(v, bound, frees, seen);
        },
        ExprKind::Index { obj, index } => {
            cfev_expr(obj, bound, frees, seen);
            cfev_expr(index, bound, frees, seen);
        }
        ExprKind::EnumCtor { args, .. } => match args {
            ilang_ast::CtorArgs::Unit => {}
            ilang_ast::CtorArgs::Tuple(es) => for e in es { cfev_expr(e, bound, frees, seen); },
            ilang_ast::CtorArgs::Struct(fs) => for (_, e) in fs { cfev_expr(e, bound, frees, seen); },
        },
        ExprKind::Match { scrutinee, arms } => {
            cfev_expr(scrutinee, bound, frees, seen);
            for arm in arms {
                let snap = bound.clone();
                cfev_pattern_binds(&arm.pattern, bound);
                cfev_expr(&arm.body, bound, frees, seen);
                *bound = snap;
            }
        }
        ExprKind::FnExpr { params, body, .. } => {
            // Inner closure: its own params shadow, but its captures
            // become OUR captures (the frees the outer closure must
            // pass through). Recurse with extended bound set.
            let snap = bound.clone();
            for p in params { bound.insert(p.name.clone()); }
            collect_fn_expr_free_vars(body, bound, frees, seen);
            *bound = snap;
        }
        ExprKind::Closure { .. } => {} // hoist hasn't run yet
    }
}

pub(super) fn cfev_pattern_binds(p: &ilang_ast::Pattern, bound: &mut std::collections::HashSet<Symbol>) {
    use ilang_ast::{PatternBindings, PatternKind};
    if let PatternKind::Variant { bindings, .. } = &p.kind {
        match bindings {
            PatternBindings::Unit => {}
            PatternBindings::Tuple(names) => for n in names {
                if n != "_" { bound.insert(n.clone()); }
            },
            PatternBindings::Struct(fs) => for (_, n) in fs {
                if n != "_" { bound.insert(n.clone()); }
            },
        }
    }
}

