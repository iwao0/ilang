//! AST walkers used by the CLI entry pipeline:
//!
//! - `collect_fn_free_var_refs`: walk every named fn / method body
//!   in the program and collect the top-level let names actually
//!   referenced as free variables. Tells the lowerer which lets
//!   have to be promoted to a host slot so cross-fn reads/writes
//!   see a single shared cell.
//! - `wrap_trailing_print`: wrap a program's trailing expression in
//!   `console.log(<tail>)` so the JIT's `__main` prints what the
//!   user expects to see.

use ilang_ast::{Expr, ExprKind, Item, Program as AstProgram, StmtKind, Symbol};


/// Walk every named fn / method body in the program and collect
/// the top-level let names actually referenced as free variables —
/// i.e. uses where no local binding (param, let in scope) of the
/// same name shadows. This is what tells the lowerer which lets
/// have to be promoted to a host slot so cross-fn reads/writes
/// see a single shared cell.
///
/// The walker tracks a stack of locally-bound names: parameters at
/// fn entry; `let` / `let-tuple` / `let-struct` bindings within a
/// block; FnExpr params when descending into closure bodies. A
/// `Var(name)` / `Assign { target: name }` only counts as
/// referencing the top-level let when the name is in `top_lets`
/// AND not in the local stack.
pub(crate) fn collect_fn_free_var_refs(
    prog: &AstProgram,
    top_lets: &std::collections::HashSet<Symbol>,
    out: &mut std::collections::HashSet<Symbol>,
) {
    let walk_class =
        |c: &ilang_ast::ClassDecl,
         out: &mut std::collections::HashSet<Symbol>| {
            for m in c.methods.iter() {
                let mut locals: Vec<Symbol> = std::iter::once(Symbol::intern("this"))
                    .chain(m.params.iter().map(|p| p.name))
                    .collect();
                walk_block(&m.body, top_lets, &mut locals, out);
            }
            for sm in c.static_methods.iter() {
                let mut locals: Vec<Symbol> =
                    sm.params.iter().map(|p| p.name).collect();
                walk_block(&sm.body, top_lets, &mut locals, out);
            }
        };
    for item in &prog.items {
        match item {
            Item::Fn(f) => {
                let mut locals: Vec<Symbol> =
                    f.params.iter().map(|p| p.name).collect();
                walk_block(&f.body, top_lets, &mut locals, out);
            }
            Item::Class(c) => walk_class(c, out),
            // `@extern(C) { ... }` and `@extern(ObjC) { ... }` blocks
            // contain inner Fn / Class items whose bodies (e.g. an
            // `@objc class` subclass override) can reference
            // top-level lets the same way a regular method can. Without
            // descending here, those references hit "unbound variable"
            // at MIR lower time because no slot got promoted.
            Item::ExternC(blk) => {
                for inner in blk.items.iter() {
                    match inner {
                        ilang_ast::ExternCItem::FnDef(f) => {
                            let mut locals: Vec<Symbol> =
                                f.params.iter().map(|p| p.name).collect();
                            walk_block(&f.body, top_lets, &mut locals, out);
                        }
                        ilang_ast::ExternCItem::Class(c) => walk_class(c, out),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    // Also descend into FnExpr bodies that appear inside top-level
    // stmts — `let f = fn(...) { ... f(...) ... }` is a
    // self-recursive closure where `f` shows up as a free variable
    // inside the FnExpr body. Walking every top-level expression
    // (not just FnExpr bodies) would over-mark refs from regular
    // __main code as "needs a slot", breaking shadowing /
    // ARC semantics for entry-file lets. Only the bodies of
    // FnExprs need promotion.
    for stmt in &prog.stmts {
        match &stmt.kind {
            StmtKind::Let { value, .. }
            | StmtKind::LetTuple { value, .. }
            | StmtKind::LetStruct { value, .. } => {
                walk_fnexpr_bodies(value, top_lets, out);
            }
            StmtKind::Expr(e) => walk_fnexpr_bodies(e, top_lets, out),
        }
    }
    if let Some(t) = &prog.tail {
        walk_fnexpr_bodies(t, top_lets, out);
    }
}

/// Recurse through `e`, collecting FnExpr bodies' free-var refs
/// against `top_lets` but NOT counting refs from non-FnExpr
/// surroundings. Distinct from `walk_expr` (which assumes we're
/// already inside a fn body, so every Var ref counts).
fn walk_fnexpr_bodies(
    e: &Expr,
    top_lets: &std::collections::HashSet<Symbol>,
    out: &mut std::collections::HashSet<Symbol>,
) {
    use ilang_ast::ExprKind as E;
    match &e.kind {
        E::FnExpr { params, body, .. } => {
            let mut locals: Vec<Symbol> = params.iter().map(|p| p.name).collect();
            walk_block(body, top_lets, &mut locals, out);
        }
        E::Unary { expr, .. }
        | E::Cast { expr, .. }
        | E::TypeTest { expr, .. }
        | E::TypeDowncast { expr, .. }
        | E::Some(expr)
        | E::Await(expr)
        | E::Field { obj: expr, .. } =>walk_fnexpr_bodies(expr, top_lets, out),
        E::Binary { lhs, rhs, .. } | E::Logical { lhs, rhs, .. } => {
            walk_fnexpr_bodies(lhs, top_lets, out);
            walk_fnexpr_bodies(rhs, top_lets, out);
        }
        E::Call { args, .. } | E::SuperCall { args, .. } | E::New { args, .. } => {
            for a in args.iter() { walk_fnexpr_bodies(a, top_lets, out); }
        }
        E::MethodCall { obj, args, .. } => {
            walk_fnexpr_bodies(obj, top_lets, out);
            for a in args.iter() { walk_fnexpr_bodies(a, top_lets, out); }
        }
        E::Block(b) => {
            for s in &b.stmts {
                match &s.kind {
                    StmtKind::Let { value, .. }
                    | StmtKind::LetTuple { value, .. }
                    | StmtKind::LetStruct { value, .. } => {
                        walk_fnexpr_bodies(value, top_lets, out);
                    }
                    StmtKind::Expr(e) => walk_fnexpr_bodies(e, top_lets, out),
                }
            }
            if let Some(t) = &b.tail { walk_fnexpr_bodies(t, top_lets, out); }
        }
        E::If { cond, then_branch, else_branch } => {
            walk_fnexpr_bodies(cond, top_lets, out);
            for s in &then_branch.stmts {
                match &s.kind {
                    StmtKind::Let { value, .. }
                    | StmtKind::LetTuple { value, .. }
                    | StmtKind::LetStruct { value, .. } => walk_fnexpr_bodies(value, top_lets, out),
                    StmtKind::Expr(e) => walk_fnexpr_bodies(e, top_lets, out),
                }
            }
            if let Some(t) = &then_branch.tail { walk_fnexpr_bodies(t, top_lets, out); }
            if let Some(e) = else_branch { walk_fnexpr_bodies(e, top_lets, out); }
        }
        E::While { cond, body } => {
            walk_fnexpr_bodies(cond, top_lets, out);
            for s in &body.stmts {
                match &s.kind {
                    StmtKind::Let { value, .. }
                    | StmtKind::LetTuple { value, .. }
                    | StmtKind::LetStruct { value, .. } => walk_fnexpr_bodies(value, top_lets, out),
                    StmtKind::Expr(e) => walk_fnexpr_bodies(e, top_lets, out),
                }
            }
            if let Some(t) = &body.tail { walk_fnexpr_bodies(t, top_lets, out); }
        }
        E::Loop { body } | E::ForIn { body, .. } => {
            for s in &body.stmts {
                match &s.kind {
                    StmtKind::Let { value, .. }
                    | StmtKind::LetTuple { value, .. }
                    | StmtKind::LetStruct { value, .. } => walk_fnexpr_bodies(value, top_lets, out),
                    StmtKind::Expr(e) => walk_fnexpr_bodies(e, top_lets, out),
                }
            }
            if let Some(t) = &body.tail { walk_fnexpr_bodies(t, top_lets, out); }
        }
        E::IfLet { expr, then_branch, else_branch, .. } => {
            walk_fnexpr_bodies(expr, top_lets, out);
            for s in &then_branch.stmts {
                match &s.kind {
                    StmtKind::Let { value, .. }
                    | StmtKind::LetTuple { value, .. }
                    | StmtKind::LetStruct { value, .. } => walk_fnexpr_bodies(value, top_lets, out),
                    StmtKind::Expr(e) => walk_fnexpr_bodies(e, top_lets, out),
                }
            }
            if let Some(t) = &then_branch.tail { walk_fnexpr_bodies(t, top_lets, out); }
            if let Some(e) = else_branch { walk_fnexpr_bodies(e, top_lets, out); }
        }
        E::Match { scrutinee, arms } => {
            walk_fnexpr_bodies(scrutinee, top_lets, out);
            for arm in arms.iter() { walk_fnexpr_bodies(&arm.body, top_lets, out); }
        }
        E::Range { start, end, .. } => {
            if let Some(s) = start { walk_fnexpr_bodies(s, top_lets, out); }
            if let Some(e) = end { walk_fnexpr_bodies(e, top_lets, out); }
        }
        E::Break(v) | E::Return(v) => {
            if let Some(e) = v { walk_fnexpr_bodies(e, top_lets, out); }
        }
        E::Array(items) | E::Tuple(items) => {
            for i in items.iter() { walk_fnexpr_bodies(i, top_lets, out); }
        }
        E::Index { obj, index } => {
            walk_fnexpr_bodies(obj, top_lets, out);
            walk_fnexpr_bodies(index, top_lets, out);
        }
        E::Assign { value, .. } => walk_fnexpr_bodies(value, top_lets, out),
        E::AssignField { obj, value, .. } => {
            walk_fnexpr_bodies(obj, top_lets, out);
            walk_fnexpr_bodies(value, top_lets, out);
        }
        E::AssignIndex { obj, index, value } => {
            walk_fnexpr_bodies(obj, top_lets, out);
            walk_fnexpr_bodies(index, top_lets, out);
            walk_fnexpr_bodies(value, top_lets, out);
        }
        E::StructLit { fields, .. } => {
            for (_, v) in fields.iter() { walk_fnexpr_bodies(v, top_lets, out); }
        }
        E::MapLit(entries) => {
            for (k, v) in entries.iter() {
                walk_fnexpr_bodies(k, top_lets, out);
                walk_fnexpr_bodies(v, top_lets, out);
            }
        }
        E::EnumCtor { args, .. } => match args {
            ilang_ast::CtorArgs::Unit => {}
            ilang_ast::CtorArgs::Tuple(es) => {
                for e in es.iter() { walk_fnexpr_bodies(e, top_lets, out); }
            }
            ilang_ast::CtorArgs::Struct(fs) => {
                for (_, e) in fs.iter() { walk_fnexpr_bodies(e, top_lets, out); }
            }
        },
        E::Var(_) | E::Closure { .. } | E::This | E::None | E::Continue
        | E::Int(_) | E::Float(_) | E::Bool(_) | E::Str(_) => {}
    }
}

fn walk_block(
    blk: &ilang_ast::Block,
    top_lets: &std::collections::HashSet<Symbol>,
    locals: &mut Vec<Symbol>,
    out: &mut std::collections::HashSet<Symbol>,
) {
    let saved = locals.len();
    for s in &blk.stmts {
        match &s.kind {
            StmtKind::Let { name, value, .. } => {
                walk_expr(value, top_lets, locals, out);
                locals.push(*name);
            }
            StmtKind::LetTuple { elems, value } => {
                walk_expr(value, top_lets, locals, out);
                for e in elems.iter().flatten() {
                    locals.push(*e);
                }
            }
            StmtKind::LetStruct { fields, value, .. } => {
                walk_expr(value, top_lets, locals, out);
                for f in fields.iter() {
                    locals.push(*f);
                }
            }
            StmtKind::Expr(e) => walk_expr(e, top_lets, locals, out),
        }
    }
    if let Some(t) = &blk.tail {
        walk_expr(t, top_lets, locals, out);
    }
    locals.truncate(saved);
}

fn walk_expr(
    e: &Expr,
    top_lets: &std::collections::HashSet<Symbol>,
    locals: &mut Vec<Symbol>,
    out: &mut std::collections::HashSet<Symbol>,
) {
    use ilang_ast::ExprKind as E;
    match &e.kind {
        E::Var(name) => {
            if top_lets.contains(name) && !locals.contains(name) {
                out.insert(*name);
            }
        }
        E::Call { callee, args } => {
            if top_lets.contains(callee) && !locals.contains(callee) {
                out.insert(*callee);
            }
            for a in args.iter() {
                walk_expr(a, top_lets, locals, out);
            }
        }
        E::Assign { target, value } => {
            if top_lets.contains(target) && !locals.contains(target) {
                out.insert(*target);
            }
            walk_expr(value, top_lets, locals, out);
        }
        E::Unary { expr, .. }
        | E::Cast { expr, .. }
        | E::TypeTest { expr, .. }
        | E::TypeDowncast { expr, .. }
        | E::Some(expr)
        | E::Await(expr)
        | E::Field { obj: expr, .. } =>walk_expr(expr, top_lets, locals, out),
        E::Binary { lhs, rhs, .. } | E::Logical { lhs, rhs, .. } => {
            walk_expr(lhs, top_lets, locals, out);
            walk_expr(rhs, top_lets, locals, out);
        }
        E::MethodCall { obj, args, .. } => {
            walk_expr(obj, top_lets, locals, out);
            for a in args.iter() {
                walk_expr(a, top_lets, locals, out);
            }
        }
        E::SuperCall { args, .. } | E::New { args, .. } => {
            for a in args.iter() {
                walk_expr(a, top_lets, locals, out);
            }
        }
        E::Block(b) => walk_block(b, top_lets, locals, out),
        E::If { cond, then_branch, else_branch } => {
            walk_expr(cond, top_lets, locals, out);
            walk_block(then_branch, top_lets, locals, out);
            if let Some(e) = else_branch {
                walk_expr(e, top_lets, locals, out);
            }
        }
        E::While { cond, body } => {
            walk_expr(cond, top_lets, locals, out);
            walk_block(body, top_lets, locals, out);
        }
        E::Loop { body } => walk_block(body, top_lets, locals, out),
        E::ForIn { var, iter, body, .. } => {
            walk_expr(iter, top_lets, locals, out);
            let saved = locals.len();
            locals.push(*var);
            walk_block(body, top_lets, locals, out);
            locals.truncate(saved);
        }
        E::IfLet { name, expr, then_branch, else_branch, .. } => {
            walk_expr(expr, top_lets, locals, out);
            let saved = locals.len();
            locals.push(*name);
            walk_block(then_branch, top_lets, locals, out);
            locals.truncate(saved);
            if let Some(e) = else_branch {
                walk_expr(e, top_lets, locals, out);
            }
        }
        E::Match { scrutinee, arms } => {
            walk_expr(scrutinee, top_lets, locals, out);
            for arm in arms.iter() {
                let saved = locals.len();
                if let ilang_ast::PatternKind::Variant { bindings, .. } = &arm.pattern.kind {
                    match bindings {
                        ilang_ast::PatternBindings::Unit => {}
                        ilang_ast::PatternBindings::Tuple(names) => {
                            for n in names.iter() {
                                locals.push(*n);
                            }
                        }
                        ilang_ast::PatternBindings::Struct(pairs) => {
                            for (_, bind) in pairs.iter() {
                                locals.push(*bind);
                            }
                        }
                    }
                }
                walk_expr(&arm.body, top_lets, locals, out);
                locals.truncate(saved);
            }
        }
        E::Range { start, end, .. } => {
            if let Some(s) = start { walk_expr(s, top_lets, locals, out); }
            if let Some(e) = end { walk_expr(e, top_lets, locals, out); }
        }
        E::Break(v) | E::Return(v) => {
            if let Some(e) = v { walk_expr(e, top_lets, locals, out); }
        }
        E::Array(items) | E::Tuple(items) => {
            for i in items.iter() { walk_expr(i, top_lets, locals, out); }
        }
        E::Index { obj, index } => {
            walk_expr(obj, top_lets, locals, out);
            walk_expr(index, top_lets, locals, out);
        }
        E::AssignField { obj, value, .. } => {
            walk_expr(obj, top_lets, locals, out);
            walk_expr(value, top_lets, locals, out);
        }
        E::AssignIndex { obj, index, value } => {
            walk_expr(obj, top_lets, locals, out);
            walk_expr(index, top_lets, locals, out);
            walk_expr(value, top_lets, locals, out);
        }
        E::StructLit { fields, .. } => {
            for (_, v) in fields.iter() { walk_expr(v, top_lets, locals, out); }
        }
        E::MapLit(entries) => {
            for (k, v) in entries.iter() {
                walk_expr(k, top_lets, locals, out);
                walk_expr(v, top_lets, locals, out);
            }
        }
        E::EnumCtor { args, .. } => match args {
            ilang_ast::CtorArgs::Unit => {}
            ilang_ast::CtorArgs::Tuple(es) => {
                for e in es.iter() { walk_expr(e, top_lets, locals, out); }
            }
            ilang_ast::CtorArgs::Struct(fs) => {
                for (_, e) in fs.iter() { walk_expr(e, top_lets, locals, out); }
            }
        },
        E::FnExpr { params, body, .. } => {
            let saved = locals.len();
            for p in params.iter() {
                locals.push(p.name);
            }
            walk_block(body, top_lets, locals, out);
            locals.truncate(saved);
        }
        E::Closure { .. } | E::This | E::None | E::Continue
        | E::Int(_) | E::Float(_) | E::Bool(_) | E::Str(_) => {}
    }
}

/// If the program has a trailing expression, wrap it in
/// `console.log(<tail>)` so the JIT path's `__main` prints the
/// value the user expected to see. Mirrors the tree-walking
/// interpreter's behaviour of returning + auto-printing the tail.
/// Programs without a tail (everything in fixture form) are
/// untouched.
pub(crate) fn wrap_trailing_print(mut prog: AstProgram) -> AstProgram {
    if let Some(tail) = prog.tail.take() {
        let span = tail.span;
        let console = Expr::new(ExprKind::Var(Symbol::intern("console")), span);
        let log_call = Expr::new(
            ExprKind::MethodCall {
                obj: Box::new(console),
                method: Symbol::intern("log"),
                args: Box::new([tail]),
            },
            span,
        );
        prog.tail = Some(log_call);
    }
    prog
}
