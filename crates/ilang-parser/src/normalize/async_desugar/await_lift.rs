//! Pre-pass that hoists every `await E` appearing inside a sub-
//! expression into its own `let __await_tN = await E` statement
//! above the use site. The state-machine builder only handles the
//! canonical "await as direct let RHS" form; this pass normalises
//! shapes like `foo(await p, await q)` or `bar(await p) + 1` into
//! that form.

use ilang_ast::{Block, Expr, ExprKind, Stmt, StmtKind, Symbol};

pub(super) fn lift_subexpr_awaits(body: Block) -> Block {
    let mut counter: u64 = 0;
    lift_block(body, &mut counter)
}

/// Block-level lift. Hoisted `let __await_tN` statements stay inside
/// the block they were found in — a nested while / if-branch block
/// keeps its awaits per-iteration / per-branch. The counter is shared
/// across the whole fn body so the synthesised names stay unique.
fn lift_block(body: Block, counter: &mut u64) -> Block {
    let mut new_stmts: Vec<Stmt> = Vec::new();
    for s in body.stmts {
        lift_stmt(s, counter, &mut new_stmts);
    }
    let new_tail = body.tail.map(|t| {
        let span = t.span;
        let mut lifts: Vec<Stmt> = Vec::new();
        // The tail is the body's value. If it's a bare `await E`,
        // we still lift it (the synthesiser expects the final tail
        // to be a sync expression — the let-await becomes the
        // body's last suspend point and the tail reduces to
        // `__await_tN`).
        let tail_lifted = lift_in_expr(*t, &mut lifts, counter, /*at_let_rhs=*/ false);
        for s in lifts {
            new_stmts.push(s);
        }
        Box::new(Expr::new(tail_lifted.kind, span))
    });
    Block { stmts: new_stmts, tail: new_tail }
}

fn lift_stmt(s: Stmt, counter: &mut u64, out: &mut Vec<Stmt>) {
    let span = s.span;
    let source_module = s.source_module.clone();
    match s.kind {
        StmtKind::Let { is_pub, is_const, name, ty, value } => {
            // Direct `let x = await E`: keep the outer await as-is,
            // but recurse into E for nested liftable awaits.
            let new_value = if let ExprKind::Await(inner) = value.kind {
                let inner_span = inner.span;
                let lifted_inner = lift_in_expr(*inner, out, counter, /*at_let_rhs=*/ true);
                Expr::new(
                    ExprKind::Await(Box::new(lifted_inner)),
                    inner_span.to(span),
                )
            } else {
                lift_in_expr(value, out, counter, /*at_let_rhs=*/ false)
            };
            let mut new_s = Stmt::new(
                StmtKind::Let { is_pub, is_const, name, ty, value: new_value },
                span,
            );
            new_s.source_module = source_module;
            out.push(new_s);
        }
        StmtKind::LetTuple { elems, value } => {
            let new_value = lift_in_expr(value, out, counter, false);
            let mut new_s = Stmt::new(StmtKind::LetTuple { elems, value: new_value }, span);
            new_s.source_module = source_module;
            out.push(new_s);
        }
        StmtKind::LetStruct { class, fields, value } => {
            let new_value = lift_in_expr(value, out, counter, false);
            let mut new_s = Stmt::new(
                StmtKind::LetStruct { class, fields, value: new_value },
                span,
            );
            new_s.source_module = source_module;
            out.push(new_s);
        }
        StmtKind::Expr(e) => {
            let new_e = lift_in_expr(e, out, counter, false);
            let mut new_s = Stmt::new(StmtKind::Expr(new_e), span);
            new_s.source_module = source_module;
            out.push(new_s);
        }
    }
}

/// Walk an expression rebuilding it; every `await E` we encounter
/// (in a position other than the *immediate* RHS of a let — handled
/// by `lift_stmt`) is replaced by `Var(__await_tN)` and an
/// `let __await_tN = await E` is appended to `lifts`. Sub-trees
/// that introduce a new scope (blocks, branches, closures) are NOT
/// descended into — awaits there are still rejected by the
/// analyser.
fn lift_in_expr(
    e: Expr,
    lifts: &mut Vec<Stmt>,
    counter: &mut u64,
    at_let_rhs: bool,
) -> Expr {
    let span = e.span;
    match e.kind {
        ExprKind::Await(inner) if !at_let_rhs => {
            // Lift this await: first recurse into the inner so a
            // nested `await E1(await E2)` becomes
            //   let __t0 = await E2
            //   let __t1 = await E1(__t0)
            // and finally the use site reads `__t1`.
            let inner_lifted = lift_in_expr(*inner, lifts, counter, false);
            let name = Symbol::intern(&format!("__await_t{}", *counter));
            *counter += 1;
            lifts.push(Stmt::new(
                StmtKind::Let {
                    is_pub: false,
                    is_const: false,
                    name,
                    ty: None,
                    value: Expr::new(
                        ExprKind::Await(Box::new(inner_lifted)),
                        span,
                    ),
                },
                span,
            ));
            Expr::new(ExprKind::Var(name), span)
        }
        ExprKind::Await(inner) => {
            // `at_let_rhs` — keep the outer await; recurse into the
            // inner for nested liftable awaits.
            let inner = lift_in_expr(*inner, lifts, counter, false);
            Expr::new(ExprKind::Await(Box::new(inner)), span)
        }
        ExprKind::Call { callee, args } => {
            let new_args: Vec<Expr> = args
                .into_vec()
                .into_iter()
                .map(|a| lift_in_expr(a, lifts, counter, false))
                .collect();
            Expr::new(
                ExprKind::Call { callee, args: new_args.into_boxed_slice() },
                span,
            )
        }
        ExprKind::MethodCall { obj, method, args } => {
            let obj = Box::new(lift_in_expr(*obj, lifts, counter, false));
            let new_args: Vec<Expr> = args
                .into_vec()
                .into_iter()
                .map(|a| lift_in_expr(a, lifts, counter, false))
                .collect();
            Expr::new(
                ExprKind::MethodCall { obj, method, args: new_args.into_boxed_slice() },
                span,
            )
        }
        ExprKind::SuperCall { method, args } => {
            let new_args: Vec<Expr> = args
                .into_vec()
                .into_iter()
                .map(|a| lift_in_expr(a, lifts, counter, false))
                .collect();
            Expr::new(
                ExprKind::SuperCall { method, args: new_args.into_boxed_slice() },
                span,
            )
        }
        ExprKind::New { class, type_args, args, init_method } => {
            let new_args: Vec<Expr> = args
                .into_vec()
                .into_iter()
                .map(|a| lift_in_expr(a, lifts, counter, false))
                .collect();
            Expr::new(
                ExprKind::New {
                    class,
                    type_args,
                    args: new_args.into_boxed_slice(),
                    init_method,
                },
                span,
            )
        }
        ExprKind::Field { obj, name } => {
            let obj = Box::new(lift_in_expr(*obj, lifts, counter, false));
            Expr::new(ExprKind::Field { obj, name }, span)
        }
        ExprKind::Index { obj, index } => {
            let obj = Box::new(lift_in_expr(*obj, lifts, counter, false));
            let index = Box::new(lift_in_expr(*index, lifts, counter, false));
            Expr::new(ExprKind::Index { obj, index }, span)
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let lhs = Box::new(lift_in_expr(*lhs, lifts, counter, false));
            let rhs = Box::new(lift_in_expr(*rhs, lifts, counter, false));
            Expr::new(ExprKind::Binary { op, lhs, rhs }, span)
        }
        ExprKind::Logical { op, lhs, rhs } => {
            // Note: `&&` / `||` short-circuit, so awaits on the
            // RHS would only run conditionally. Lifting them
            // unconditionally changes semantics. For safety, we
            // DON'T lift into Logical's rhs; if a user writes
            // `cond && await p`, the analyser still rejects the
            // nested await. (Lifting only the lhs is sound.)
            let lhs = Box::new(lift_in_expr(*lhs, lifts, counter, false));
            Expr::new(ExprKind::Logical { op, lhs, rhs }, span)
        }
        ExprKind::Unary { op, expr } => {
            let expr = Box::new(lift_in_expr(*expr, lifts, counter, false));
            Expr::new(ExprKind::Unary { op, expr }, span)
        }
        ExprKind::Cast { expr, ty } => {
            let expr = Box::new(lift_in_expr(*expr, lifts, counter, false));
            Expr::new(ExprKind::Cast { expr, ty }, span)
        }
        ExprKind::TypeTest { expr, ty } => {
            let expr = Box::new(lift_in_expr(*expr, lifts, counter, false));
            Expr::new(ExprKind::TypeTest { expr, ty }, span)
        }
        ExprKind::TypeDowncast { expr, ty } => {
            let expr = Box::new(lift_in_expr(*expr, lifts, counter, false));
            Expr::new(ExprKind::TypeDowncast { expr, ty }, span)
        }
        ExprKind::Some(inner) => {
            let inner = Box::new(lift_in_expr(*inner, lifts, counter, false));
            Expr::new(ExprKind::Some(inner), span)
        }
        ExprKind::Tuple(es) => {
            let new_es: Vec<Expr> = es
                .into_vec()
                .into_iter()
                .map(|x| lift_in_expr(x, lifts, counter, false))
                .collect();
            Expr::new(ExprKind::Tuple(new_es.into_boxed_slice()), span)
        }
        ExprKind::Array(es) => {
            let new_es: Vec<Expr> = es
                .into_vec()
                .into_iter()
                .map(|x| lift_in_expr(x, lifts, counter, false))
                .collect();
            Expr::new(ExprKind::Array(new_es.into_boxed_slice()), span)
        }
        ExprKind::Return(opt) => {
            let new_opt = opt.map(|e| Box::new(lift_in_expr(*e, lifts, counter, false)));
            Expr::new(ExprKind::Return(new_opt), span)
        }
        // Assignments: lift awaits out of the RHS (and the receiver /
        // index sub-exprs) so `total = total + await p` becomes
        //   let __await_t0 = await p
        //   total = total + __await_t0
        // Without these arms the await stayed nested, the segment
        // builder saw a single segment, and lower_async_fn hit its
        // "NoAwait after body_contains_await" panic.
        ExprKind::Assign { target, value } => {
            let value = Box::new(lift_in_expr(*value, lifts, counter, false));
            Expr::new(ExprKind::Assign { target, value }, span)
        }
        ExprKind::AssignField { obj, field, value, is_init } => {
            let obj = Box::new(lift_in_expr(*obj, lifts, counter, false));
            let value = Box::new(lift_in_expr(*value, lifts, counter, false));
            Expr::new(ExprKind::AssignField { obj, field, value, is_init }, span)
        }
        ExprKind::AssignIndex { obj, index, value } => {
            let obj = Box::new(lift_in_expr(*obj, lifts, counter, false));
            let index = Box::new(lift_in_expr(*index, lifts, counter, false));
            let value = Box::new(lift_in_expr(*value, lifts, counter, false));
            Expr::new(ExprKind::AssignIndex { obj, index, value }, span)
        }
        // `if` cond and `match` scrutinee are evaluated
        // unconditionally before any arm runs, so we CAN lift
        // awaits there. The arm bodies themselves are NOT
        // descended into — those are scope-specific and the
        // state-machine handles awaits inside them via separate
        // dispatch. Same logic doesn't apply to `while` cond
        // (re-evaluated each iter) — leave those alone.
        ExprKind::If { cond, then_branch, else_branch } => {
            let cond = Box::new(lift_in_expr(*cond, lifts, counter, false));
            // Branch blocks lift in place — hoisted lets stay inside
            // their branch, so the awaits remain conditional. The
            // else side is an Expr (block or elif chain); both shapes
            // recurse through the matching arms here.
            let then_branch = lift_block(then_branch, counter);
            let else_branch =
                else_branch.map(|e| Box::new(lift_in_expr(*e, lifts, counter, false)));
            Expr::new(
                ExprKind::If { cond, then_branch, else_branch },
                span,
            )
        }
        ExprKind::Match { scrutinee, arms } => {
            let scrutinee = Box::new(lift_in_expr(*scrutinee, lifts, counter, false));
            // Block-bodied arms lift in place (per-arm scope). Bare
            // expression arms stay untouched — hoisting those into
            // the surrounding block would evaluate them
            // unconditionally.
            let arms: Vec<_> = arms
                .into_vec()
                .into_iter()
                .map(|mut a| {
                    if let ExprKind::Block(b) = a.body.kind {
                        let b_span = a.body.span;
                        a.body = Expr::new(
                            ExprKind::Block(lift_block(b, counter)),
                            b_span,
                        );
                    }
                    a
                })
                .collect();
            Expr::new(
                ExprKind::Match { scrutinee, arms: arms.into_boxed_slice() },
                span,
            )
        }
        // Loop-shaped bodies are their own blocks: lift inside them
        // (the hoisted lets stay per-iteration). The while cond is
        // re-evaluated every lap, so awaits there are left alone —
        // the segment builder rejects await-bearing conds.
        ExprKind::While { cond, body } => {
            let body = lift_block(body, counter);
            Expr::new(ExprKind::While { cond, body }, span)
        }
        ExprKind::Loop { body } => {
            let body = lift_block(body, counter);
            Expr::new(ExprKind::Loop { body }, span)
        }
        ExprKind::ForIn { var, iter, body } => {
            let body = lift_block(body, counter);
            Expr::new(ExprKind::ForIn { var, iter, body }, span)
        }
        ExprKind::Block(b) => {
            Expr::new(ExprKind::Block(lift_block(b, counter)), span)
        }
        // Everything else that introduces a new scope (closures) is
        // NOT descended into. Awaits inside are rejected by the
        // analyser.
        kind => Expr::new(kind, span),
    }
}
