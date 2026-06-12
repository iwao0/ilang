//! Pre-passes that walk an AST block / expression and collect names
//! the lowerer needs to know about up front:
//!
//! - `collect_cellified_names_*`: bindings captured by a closure and
//!   subsequently re-assigned — those must be boxed into a heap
//!   cell so the inner closure and the outer scope see the same
//!   underlying slot.
//! - `collect_mut_assigned_*`: bindings whose name appears on the LHS
//!   of an `Assign`. These are promoted to Cranelift `Variable`s
//!   (mutable locals); un-mutated `let`s stay as plain SSA values.
//! - `collect_free_vars_*`: names referenced inside a block but not
//!   bound by it — drives the closure capture layout.

use ilang_ast::{self as ast, Expr, ExprKind, Stmt, StmtKind, Symbol};

pub(super) fn collect_cellified_names_stmt(
    stmt: &Stmt,
    out: &mut std::collections::HashSet<Symbol>,
) {
    match &stmt.kind {
        StmtKind::Let { value, .. } => collect_cellified_names_expr(value, out),
        StmtKind::LetTuple { value, .. } | StmtKind::LetStruct { value, .. } => {
            collect_cellified_names_expr(value, out)
        }
        StmtKind::Expr(e) => collect_cellified_names_expr(e, out),
    }
}

pub(super) fn collect_cellified_names_block(
    body: &ast::Block,
    out: &mut std::collections::HashSet<Symbol>,
) {
    for stmt in &body.stmts {
        match &stmt.kind {
            StmtKind::Let { value, .. } => collect_cellified_names_expr(value, out),
            StmtKind::LetTuple { value, .. } | StmtKind::LetStruct { value, .. } => {
                collect_cellified_names_expr(value, out)
            }
            StmtKind::Expr(e) => collect_cellified_names_expr(e, out),
        }
    }
    if let Some(t) = &body.tail {
        collect_cellified_names_expr(t, out);
    }
}

pub(super) fn collect_cellified_names_expr(
    expr: &Expr,
    out: &mut std::collections::HashSet<Symbol>,
) {
    use ExprKind as E;
    match &expr.kind {
        E::FnExpr { params, body, .. } => {
            // Names assigned inside the closure body, minus params.
            let mut bound: std::collections::HashSet<Symbol> =
                params.iter().map(|p| p.name).collect();
            let mut frees: Vec<Symbol> = Vec::new();
            collect_free_vars_block(body, &mut bound, &mut frees);
            let mut assigned: std::collections::HashSet<Symbol> =
                std::collections::HashSet::new();
            collect_mut_assigned_block(body, &mut assigned);
            for name in frees {
                if assigned.contains(&name) {
                    out.insert(name);
                }
            }
            // Also recurse into the body so nested FnExprs cellify
            // their own captures.
            collect_cellified_names_block(body, out);
        }
        // Recurse into composite forms.
        E::Block(b) => collect_cellified_names_block(b, out),
        E::If { cond, then_branch, else_branch } => {
            collect_cellified_names_expr(cond, out);
            collect_cellified_names_block(then_branch, out);
            if let Some(e) = else_branch {
                collect_cellified_names_expr(e, out);
            }
        }
        E::While { cond, body } => {
            collect_cellified_names_expr(cond, out);
            collect_cellified_names_block(body, out);
        }
        E::Loop { body } => collect_cellified_names_block(body, out),
        E::ForIn { iter, body, .. } => {
            collect_cellified_names_expr(iter, out);
            collect_cellified_names_block(body, out);
        }
        E::IfLet { expr, then_branch, else_branch, .. } => {
            collect_cellified_names_expr(expr, out);
            collect_cellified_names_block(then_branch, out);
            if let Some(e) = else_branch {
                collect_cellified_names_expr(e, out);
            }
        }
        E::Match { scrutinee, arms } => {
            collect_cellified_names_expr(scrutinee, out);
            for arm in arms.iter() {
                collect_cellified_names_expr(&arm.body, out);
            }
        }
        E::Unary { expr, .. }
        | E::Cast { expr, .. }
        | E::TypeTest { expr, .. }
        | E::TypeDowncast { expr, .. }
        | E::Some(expr)
        | E::Await(expr)
        | E::Field { obj: expr, .. } => collect_cellified_names_expr(expr, out),
        E::Binary { lhs, rhs, .. } | E::Logical { lhs, rhs, .. } => {
            collect_cellified_names_expr(lhs, out);
            collect_cellified_names_expr(rhs, out);
        }
        E::Call { args, .. } | E::New { args, .. } | E::SuperCall { args, .. } => {
            for a in args.iter() {
                collect_cellified_names_expr(a, out);
            }
        }
        E::MethodCall { obj, args, .. } => {
            collect_cellified_names_expr(obj, out);
            for a in args.iter() {
                collect_cellified_names_expr(a, out);
            }
        }
        E::Range { start, end, .. } => {
            if let Some(s) = start {
                collect_cellified_names_expr(s, out);
            }
            if let Some(e) = end {
                collect_cellified_names_expr(e, out);
            }
        }
        E::Break(v) | E::Return(v) => {
            if let Some(e) = v {
                collect_cellified_names_expr(e, out);
            }
        }
        E::Assign { value, .. } => collect_cellified_names_expr(value, out),
        E::AssignField { obj, value, .. } => {
            collect_cellified_names_expr(obj, out);
            collect_cellified_names_expr(value, out);
        }
        E::Array(items) | E::Tuple(items) => {
            for i in items.iter() {
                collect_cellified_names_expr(i, out);
            }
        }
        E::StructLit { fields, .. } => {
            for (_, v) in fields.iter() {
                collect_cellified_names_expr(v, out);
            }
        }
        E::MapLit(entries) => {
            for (k, v) in entries.iter() {
                collect_cellified_names_expr(k, out);
                collect_cellified_names_expr(v, out);
            }
        }
        E::Index { obj, index } => {
            collect_cellified_names_expr(obj, out);
            collect_cellified_names_expr(index, out);
        }
        E::AssignIndex { obj, index, value } => {
            collect_cellified_names_expr(obj, out);
            collect_cellified_names_expr(index, out);
            collect_cellified_names_expr(value, out);
        }
        E::EnumCtor { args, .. } => match args {
            ast::CtorArgs::Unit => {}
            ast::CtorArgs::Tuple(es) => {
                for e in es.iter() {
                    collect_cellified_names_expr(e, out);
                }
            }
            ast::CtorArgs::Struct(named) => {
                for (_, e) in named.iter() {
                    collect_cellified_names_expr(e, out);
                }
            }
        },
        _ => {}
    }
}

/// Pre-pass: walk a fn body to find every `Assign { target }` site.
/// Names that show up here are treated as mutable locals (Cranelift
/// `Variable`s) by the lowerer; un-mutated `let` bindings stay as
/// plain SSA values.
pub(super) fn collect_mut_assigned_block(
    body: &ast::Block,
    out: &mut std::collections::HashSet<Symbol>,
) {
    for stmt in &body.stmts {
        match &stmt.kind {
            StmtKind::Let { value, .. } => collect_mut_assigned_expr(value, out),
            StmtKind::LetTuple { value, .. } | StmtKind::LetStruct { value, .. } => {
                collect_mut_assigned_expr(value, out)
            }
            StmtKind::Expr(e) => collect_mut_assigned_expr(e, out),
        }
    }
    if let Some(t) = &body.tail {
        collect_mut_assigned_expr(t, out);
    }
}

pub(super) fn collect_mut_assigned_expr(expr: &Expr, out: &mut std::collections::HashSet<Symbol>) {
    use ExprKind as E;
    match &expr.kind {
        E::Assign { target, value } => {
            out.insert(*target);
            collect_mut_assigned_expr(value, out);
        }
        E::Int(_) | E::Float(_) | E::Bool(_) | E::Str(_) | E::Var(_) | E::This | E::None | E::Continue => {}
        E::Unary { expr, .. } | E::Cast { expr, .. } | E::TypeTest { expr, .. }
        | E::TypeDowncast { expr, .. } | E::Some(expr) | E::Await(expr)
        | E::Field { obj: expr, .. } => {
            collect_mut_assigned_expr(expr, out)
        }
        E::Binary { lhs, rhs, .. } | E::Logical { lhs, rhs, .. } => {
            collect_mut_assigned_expr(lhs, out);
            collect_mut_assigned_expr(rhs, out);
        }
        E::Call { args, .. } | E::New { args, .. } | E::SuperCall { args, .. } => {
            for a in args.iter() {
                collect_mut_assigned_expr(a, out);
            }
        }
        E::MethodCall { obj, args, .. } => {
            collect_mut_assigned_expr(obj, out);
            for a in args.iter() {
                collect_mut_assigned_expr(a, out);
            }
        }
        E::Block(b) => collect_mut_assigned_block(b, out),
        E::If { cond, then_branch, else_branch } => {
            collect_mut_assigned_expr(cond, out);
            collect_mut_assigned_block(then_branch, out);
            if let Some(e) = else_branch {
                collect_mut_assigned_expr(e, out);
            }
        }
        E::While { cond, body } => {
            collect_mut_assigned_expr(cond, out);
            collect_mut_assigned_block(body, out);
        }
        E::ForIn { iter, body, .. } => {
            collect_mut_assigned_expr(iter, out);
            collect_mut_assigned_block(body, out);
        }
        E::Loop { body } => collect_mut_assigned_block(body, out),
        E::Range { start, end, .. } => {
            if let Some(s) = start {
                collect_mut_assigned_expr(s, out);
            }
            if let Some(e) = end {
                collect_mut_assigned_expr(e, out);
            }
        }
        E::Break(v) | E::Return(v) => {
            if let Some(e) = v {
                collect_mut_assigned_expr(e, out);
            }
        }
        E::AssignField { obj, value, .. } => {
            collect_mut_assigned_expr(obj, out);
            collect_mut_assigned_expr(value, out);
        }
        E::Array(items) | E::Tuple(items) => {
            for i in items.iter() {
                collect_mut_assigned_expr(i, out);
            }
        }
        E::StructLit { fields, .. } => {
            for (_, v) in fields.iter() {
                collect_mut_assigned_expr(v, out);
            }
        }
        E::MapLit(entries) => {
            for (k, v) in entries.iter() {
                collect_mut_assigned_expr(k, out);
                collect_mut_assigned_expr(v, out);
            }
        }
        E::Index { obj, index } => {
            collect_mut_assigned_expr(obj, out);
            collect_mut_assigned_expr(index, out);
        }
        E::AssignIndex { obj, index, value } => {
            collect_mut_assigned_expr(obj, out);
            collect_mut_assigned_expr(index, out);
            collect_mut_assigned_expr(value, out);
        }
        E::IfLet { expr, then_branch, else_branch, .. } => {
            collect_mut_assigned_expr(expr, out);
            collect_mut_assigned_block(then_branch, out);
            if let Some(e) = else_branch {
                collect_mut_assigned_expr(e, out);
            }
        }
        E::EnumCtor { args, .. } => match args {
            ast::CtorArgs::Unit => {}
            ast::CtorArgs::Tuple(es) => {
                for e in es.iter() {
                    collect_mut_assigned_expr(e, out);
                }
            }
            ast::CtorArgs::Struct(named) => {
                for (_, e) in named.iter() {
                    collect_mut_assigned_expr(e, out);
                }
            }
        },
        E::Match { scrutinee, arms } => {
            collect_mut_assigned_expr(scrutinee, out);
            for arm in arms.iter() {
                collect_mut_assigned_expr(&arm.body, out);
            }
        }
        E::FnExpr { body, .. } => collect_mut_assigned_block(body, out),
        E::Closure { .. } => {}
        E::Template { parts } => {
            for p in parts.iter() {
                if let ast::TemplatePart::Expr(e) = p {
                    collect_mut_assigned_expr(e, out);
                }
            }
        }
    }
}

/// Collect names referenced in `body` but not bound by it. `bound`
/// tracks names introduced by enclosing parameters / lets so they
/// don't show up as captures. The output `frees` may contain
/// duplicates; the caller dedups when building the env layout.
pub(super) fn collect_free_vars_block(
    body: &ast::Block,
    bound: &mut std::collections::HashSet<Symbol>,
    frees: &mut Vec<Symbol>,
) {
    let snapshot = bound.clone();
    for stmt in &body.stmts {
        match &stmt.kind {
            StmtKind::Let { name, value, .. } => {
                collect_free_vars_expr(value, bound, frees);
                bound.insert(*name);
            }
            StmtKind::LetTuple { elems, value } => {
                collect_free_vars_expr(value, bound, frees);
                for n in elems.iter().flatten() {
                    bound.insert(*n);
                }
            }
            StmtKind::LetStruct { fields, value, .. } => {
                collect_free_vars_expr(value, bound, frees);
                for n in fields.iter() {
                    bound.insert(*n);
                }
            }
            StmtKind::Expr(e) => collect_free_vars_expr(e, bound, frees),
        }
    }
    if let Some(t) = &body.tail {
        collect_free_vars_expr(t, bound, frees);
    }
    *bound = snapshot;
}

pub(super) fn collect_free_vars_expr(
    expr: &Expr,
    bound: &mut std::collections::HashSet<Symbol>,
    frees: &mut Vec<Symbol>,
) {
    use ExprKind as E;
    match &expr.kind {
        E::Int(_) | E::Float(_) | E::Bool(_) | E::Str(_) | E::None | E::Continue => {}
        E::This => {
            // `this` referenced inside a closure body should capture
            // the enclosing method's receiver.
            let n = Symbol::intern("this");
            if !bound.contains(&n) && !frees.contains(&n) {
                frees.push(n);
            }
        }
        E::Var(n) => {
            if !bound.contains(n) && !frees.contains(n) {
                frees.push(*n);
            }
            // A bare `n` inside a class method body may desugar to
            // `this.n` when `n` is a field / method. `this` is NOT
            // added speculatively here — that over-captured the
            // receiver for closures that only reference a plain local
            // / param (`fn(){ new Box(x) }`), creating a `this →
            // closure → this` cycle when the closure was stored in a
            // field of the receiver. `lower_fn_expr` captures `this`
            // on demand instead, only when a free var actually resolves
            // to a member of the enclosing class.
        }
        E::Unary { expr, .. } => collect_free_vars_expr(expr, bound, frees),
        E::Binary { lhs, rhs, .. } | E::Logical { lhs, rhs, .. } => {
            collect_free_vars_expr(lhs, bound, frees);
            collect_free_vars_expr(rhs, bound, frees);
        }
        E::Call { callee, args } => {
            // Bare-name calls might target a captured fn-typed local
            // (`compose(f,g) { fn(x){ f(g(x)) } }`). Treat the callee
            // as a potential free var. The lower_fn_expr loop filters
            // out names not bound in the surrounding env, so global
            // top-level fns simply pass through unchanged.
            if !bound.contains(callee) && !frees.contains(callee) {
                frees.push(*callee);
            }
            for a in args.iter() {
                collect_free_vars_expr(a, bound, frees);
            }
        }
        E::New { args, .. } => {
            for a in args.iter() {
                collect_free_vars_expr(a, bound, frees);
            }
        }
        E::SuperCall { args, .. } => {
            // `super.method(...)` implicitly references `this`.
            let this_sym = Symbol::intern("this");
            if !bound.contains(&this_sym) && !frees.contains(&this_sym) {
                frees.push(this_sym);
            }
            for a in args.iter() {
                collect_free_vars_expr(a, bound, frees);
            }
        }
        E::Field { obj, .. } => collect_free_vars_expr(obj, bound, frees),
        E::MethodCall { obj, args, .. } => {
            collect_free_vars_expr(obj, bound, frees);
            for a in args.iter() {
                collect_free_vars_expr(a, bound, frees);
            }
        }
        E::Block(b) => collect_free_vars_block(b, bound, frees),
        E::If { cond, then_branch, else_branch } => {
            collect_free_vars_expr(cond, bound, frees);
            collect_free_vars_block(then_branch, bound, frees);
            if let Some(e) = else_branch {
                collect_free_vars_expr(e, bound, frees);
            }
        }
        E::While { cond, body } => {
            collect_free_vars_expr(cond, bound, frees);
            collect_free_vars_block(body, bound, frees);
        }
        E::ForIn { var, iter, body } => {
            collect_free_vars_expr(iter, bound, frees);
            let saved = bound.clone();
            bound.insert(*var);
            collect_free_vars_block(body, bound, frees);
            *bound = saved;
        }
        E::Range { start, end, .. } => {
            if let Some(s) = start {
                collect_free_vars_expr(s, bound, frees);
            }
            if let Some(e) = end {
                collect_free_vars_expr(e, bound, frees);
            }
        }
        E::Closure { captures, .. } => {
            for (n, _) in captures.iter() {
                if !bound.contains(n) && !frees.contains(n) {
                    frees.push(*n);
                }
            }
        }
        E::Loop { body } => collect_free_vars_block(body, bound, frees),
        E::Break(v) | E::Return(v) => {
            if let Some(e) = v {
                collect_free_vars_expr(e, bound, frees);
            }
        }
        E::Assign { value, target } => {
            collect_free_vars_expr(value, bound, frees);
            if !bound.contains(target) && !frees.contains(target) {
                frees.push(*target);
            }
        }
        E::AssignField { obj, value, .. } => {
            collect_free_vars_expr(obj, bound, frees);
            collect_free_vars_expr(value, bound, frees);
        }
        E::Cast { expr, .. } | E::TypeTest { expr, .. } | E::TypeDowncast { expr, .. } => {
            collect_free_vars_expr(expr, bound, frees);
        }
        E::FnExpr { params, body, .. } => {
            // Inner closure: its own params shadow, then the body's
            // free-vars are the outer's captures (minus its params).
            let saved = bound.clone();
            for p in params.iter() {
                bound.insert(p.name);
            }
            collect_free_vars_block(body, bound, frees);
            *bound = saved;
        }
        E::Array(items) | E::Tuple(items) => {
            for i in items.iter() {
                collect_free_vars_expr(i, bound, frees);
            }
        }
        E::StructLit { fields, .. } => {
            for (_, v) in fields.iter() {
                collect_free_vars_expr(v, bound, frees);
            }
        }
        E::MapLit(entries) => {
            for (k, v) in entries.iter() {
                collect_free_vars_expr(k, bound, frees);
                collect_free_vars_expr(v, bound, frees);
            }
        }
        E::Index { obj, index } => {
            collect_free_vars_expr(obj, bound, frees);
            collect_free_vars_expr(index, bound, frees);
        }
        E::AssignIndex { obj, index, value } => {
            collect_free_vars_expr(obj, bound, frees);
            collect_free_vars_expr(index, bound, frees);
            collect_free_vars_expr(value, bound, frees);
        }
        E::Some(e) | E::Await(e) => collect_free_vars_expr(e, bound, frees),
        E::IfLet { name, expr, then_branch, else_branch } => {
            collect_free_vars_expr(expr, bound, frees);
            let saved = bound.clone();
            bound.insert(*name);
            collect_free_vars_block(then_branch, bound, frees);
            *bound = saved;
            if let Some(e) = else_branch {
                collect_free_vars_expr(e, bound, frees);
            }
        }
        E::EnumCtor { args, .. } => match args {
            ast::CtorArgs::Unit => {}
            ast::CtorArgs::Tuple(es) => {
                for e in es.iter() {
                    collect_free_vars_expr(e, bound, frees);
                }
            }
            ast::CtorArgs::Struct(named) => {
                for (_, e) in named.iter() {
                    collect_free_vars_expr(e, bound, frees);
                }
            }
        },
        E::Match { scrutinee, arms } => {
            collect_free_vars_expr(scrutinee, bound, frees);
            for arm in arms.iter() {
                let saved = bound.clone();
                if let ast::PatternKind::Variant { bindings, .. } = &arm.pattern.kind {
                    match bindings {
                        ast::PatternBindings::Tuple(names) => {
                            for n in names.iter() {
                                bound.insert(*n);
                            }
                        }
                        ast::PatternBindings::Struct(named) => {
                            for (_, b) in named.iter() {
                                bound.insert(*b);
                            }
                        }
                        ast::PatternBindings::Unit => {}
                    }
                }
                collect_free_vars_expr(&arm.body, bound, frees);
                *bound = saved;
            }
        }
        E::Template { parts } => {
            for p in parts.iter() {
                if let ast::TemplatePart::Expr(e) = p {
                    collect_free_vars_expr(e, bound, frees);
                }
            }
        }
    }
}
