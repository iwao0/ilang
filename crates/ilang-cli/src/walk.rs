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

use std::collections::HashMap;

use ilang_ast::{Expr, ExprKind, Item, Program as AstProgram, StmtKind, Symbol};

/// The in-scope local names during a free-var walk.
///
/// Backed by an insertion-ordered `Vec` (so a block exit can rewind to
/// a saved length, the way lexical scopes nest) plus a `HashMap` of
/// live shadow counts keyed by name. The map turns the hot
/// `contains` check — run at every `Var` / `Call` / `Assign` — from a
/// linear scan of the `Vec` into an O(1) lookup, which matters in deep
/// scopes or large functions where the old `Vec::contains` made the
/// walk O(exprs × locals).
///
/// The two stay in lock-step: `push` appends to the `Vec` and bumps the
/// name's count; `truncate` pops the tail back to `len` and drops each
/// popped name's count (removing the entry at zero). A name shadowed N
/// times has count N, so it stays "in scope" until the last shadow is
/// popped.
#[derive(Default)]
struct Scope {
    order: Vec<Symbol>,
    counts: HashMap<Symbol, u32>,
}

impl Scope {
    fn len(&self) -> usize {
        self.order.len()
    }

    fn push(&mut self, name: Symbol) {
        self.order.push(name);
        *self.counts.entry(name).or_insert(0) += 1;
    }

    fn truncate(&mut self, len: usize) {
        while self.order.len() > len {
            let name = self.order.pop().expect("len checked above");
            if let std::collections::hash_map::Entry::Occupied(mut e) =
                self.counts.entry(name)
            {
                let c = e.get_mut();
                *c -= 1;
                if *c == 0 {
                    e.remove();
                }
            }
        }
    }

    fn contains(&self, name: &Symbol) -> bool {
        self.counts.contains_key(name)
    }
}

impl FromIterator<Symbol> for Scope {
    fn from_iter<I: IntoIterator<Item = Symbol>>(iter: I) -> Self {
        let mut s = Scope::default();
        for name in iter {
            s.push(name);
        }
        s
    }
}

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
                let mut locals: Scope = std::iter::once(Symbol::intern("this"))
                    .chain(m.params.iter().map(|p| p.name))
                    .collect();
                walk_block(&m.body, top_lets, &mut locals, out);
            }
            for sm in c.static_methods.iter() {
                let mut locals: Scope =
                    sm.params.iter().map(|p| p.name).collect();
                walk_block(&sm.body, top_lets, &mut locals, out);
            }
        };
    for item in &prog.items {
        match item {
            Item::Fn(f) => {
                let mut locals: Scope =
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
                            let mut locals: Scope =
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
///
/// The non-FnExpr arms only need to descend into children looking
/// for more FnExpr boundaries — that traversal is shared with the
/// `walk_expr_descendants_ref` skeleton in `ilang_ast::walk`.
fn walk_fnexpr_bodies(
    e: &Expr,
    top_lets: &std::collections::HashSet<Symbol>,
    out: &mut std::collections::HashSet<Symbol>,
) {
    if let ExprKind::FnExpr { params, body, .. } = &e.kind {
        let mut locals: Scope = params.iter().map(|p| p.name).collect();
        walk_block(body, top_lets, &mut locals, out);
        return;
    }
    // The callback is infallible; pin `E = ()` and discard the Ok.
    let _: Result<(), ()> = ilang_ast::walk::walk_expr_descendants_ref(e, &mut |child| {
        walk_fnexpr_bodies(child, top_lets, out);
        Ok(())
    });
}

fn walk_block(
    blk: &ilang_ast::Block,
    top_lets: &std::collections::HashSet<Symbol>,
    locals: &mut Scope,
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
    locals: &mut Scope,
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
        E::Template { parts } => {
            for p in parts.iter() {
                if let ilang_ast::TemplatePart::Expr(e2) = p {
                    walk_expr(e2, top_lets, locals, out);
                }
            }
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
