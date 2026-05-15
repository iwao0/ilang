//! `async fn` → enum-variant state machine (Phase 1 of the
//! principled lowering — straight-line bodies only).
//!
//! ## Design
//!
//! For an async fn with N `await` sites, the body splits into
//! N+1 **segments** (code between awaits). The state lives as a
//! heap-allocated enum value with one variant per segment; each
//! variant carries exactly the locals live at that segment's
//! entry (params + previously-introduced let bindings). A small
//! wrapper class (`__<name>_StateRef`) holds the enum cell + the
//! result Promise pointer so the continuation closures registered
//! with `.then` can mutate the state across the chain.
//!
//! Compared to the legacy class-based scheme, this removes the
//! "all fields must be assigned in init" problem (each variant
//! carries only fields it needs) and aligns with Rust's
//! `Generator` lowering.
//!
//! ## Generated layout (sketch)
//!
//! ```ignore
//! async fn run(p: Promise<i64>): i64 {
//!     let c = new Counter(10)
//!     let v = await p
//!     c.n + v
//! }
//! ```
//!
//! lowers to:
//!
//! ```ignore
//! enum __run_State {
//!     S0: { p: Promise<i64> }
//!     S1: { p: Promise<i64>, c: Counter, v: i64 }
//! }
//!
//! class __run_StateRef {
//!     pub current: __run_State
//!     pub __async_promise: Promise<i64>
//!     pub init(initial: __run_State, prom: Promise<i64>) {
//!         this.current = initial
//!         this.__async_promise = prom
//!     }
//! }
//!
//! fn __run_poll(state_ref: __run_StateRef, _awaited: i64) {
//!     loop {
//!         match state_ref.current {
//!             __run_State.S0 { p } {
//!                 let c = new Counter(10)
//!                 let _ = p.then(fn(v: i64): i64 {
//!                     state_ref.current = __run_State.S1 { p: p, c: c, v: v }
//!                     __run_poll(state_ref, 0)
//!                     v
//!                 })
//!                 return
//!             }
//!             __run_State.S1 { p, c, v } {
//!                 Promise.__settleResolve(state_ref.__async_promise, c.n + v)
//!                 return
//!             }
//!         }
//!     }
//! }
//!
//! fn run(p: Promise<i64>): Promise<i64> {
//!     let prom = Promise.__pending()
//!     let initial = __run_State.S0 { p: p }
//!     let state_ref = new __run_StateRef(initial, prom)
//!     __run_poll(state_ref, 0)
//!     prom
//! }
//! ```
//!
//! ## Scope (Phase 1)
//!
//! Only straight-line bodies — let-await as the suspend boundary,
//! plus optional sync stmts between awaits and a tail expression
//! on the final segment. Bodies containing `if` / `while` /
//! `match` fall back to the legacy class-based lowering until
//! Phase 2 migrates them.

use std::collections::HashMap;

use ilang_ast::{
    Block, ClassDecl, CtorArgs, EnumDecl, Expr, ExprKind, FieldDecl, FnDecl, MatchArm,
    Param, Pattern, PatternBindings, PatternKind, Span, Stmt, StmtKind, Symbol, Type,
    Variant, VariantPayload,
};

/// Wrap an arbitrary `Expr` as a `Block` whose tail is that expr
/// (when the expr isn't already a `Block`). Used to normalize
/// `else if` chains and bare expression else / arm bodies into the
/// uniform Block shape that `build_block` walks.
fn coerce_to_block(e: &Expr) -> Block {
    match &e.kind {
        ExprKind::Block(b) => b.clone(),
        _ => Block {
            stmts: Vec::new(),
            tail: Some(Box::new(e.clone())),
        },
    }
}

/// Returns Some(()) if `e` is a `let X = <if|match>` shape whose
/// branches / arms contain awaits — i.e. it needs the mid-body
/// join lowering rather than the simple sync-let path.
fn mid_body_join_kind(e: &Expr) -> Option<()> {
    match &e.kind {
        ExprKind::If { cond, then_branch, else_branch } => {
            if expr_has_await(cond) {
                return None;
            }
            let has = block_has_await(then_branch)
                || else_branch.as_deref().is_some_and(expr_has_await);
            if has { Some(()) } else { None }
        }
        ExprKind::Match { scrutinee, arms } => {
            if expr_has_await(scrutinee) {
                return None;
            }
            let has = arms.iter().any(|a| expr_has_await(&a.body));
            if has { Some(()) } else { None }
        }
        _ => None,
    }
}

// --- `loop { body }` → `while true { body }` ---------------------
//
// Reuses the existing while-with-await machinery (cond is await-
// free `true`, body inherits the user's break/continue intent).
// Sync loops don't need rewriting — only those whose body contains
// awaits would otherwise be rejected as unsupported.

pub fn desugar_loop_to_while(mut body: Block) -> Block {
    desugar_loop_in_block(&mut body);
    body
}

fn desugar_loop_in_block(b: &mut Block) {
    for s in b.stmts.iter_mut() {
        desugar_loop_in_stmt(s);
    }
    if let Some(t) = b.tail.as_mut() {
        desugar_loop_in_expr(t);
    }
}

fn desugar_loop_in_stmt(s: &mut Stmt) {
    match &mut s.kind {
        StmtKind::Let { value, .. }
        | StmtKind::LetTuple { value, .. }
        | StmtKind::LetStruct { value, .. } => desugar_loop_in_expr(value),
        StmtKind::Expr(e) => desugar_loop_in_expr(e),
    }
}

fn desugar_loop_in_expr(e: &mut Expr) {
    match &mut e.kind {
        ExprKind::Loop { body } => {
            desugar_loop_in_block(body);
            if block_has_await(body) {
                let span = e.span;
                let new_body = std::mem::replace(body, Block { stmts: Vec::new(), tail: None });
                *e = Expr::new(
                    ExprKind::While {
                        cond: Box::new(Expr::new(ExprKind::Bool(true), span)),
                        body: new_body,
                    },
                    span,
                );
            }
        }
        ExprKind::Block(b) => desugar_loop_in_block(b),
        ExprKind::If { cond, then_branch, else_branch } => {
            desugar_loop_in_expr(cond);
            desugar_loop_in_block(then_branch);
            if let Some(eb) = else_branch {
                desugar_loop_in_expr(eb);
            }
        }
        ExprKind::IfLet { expr, then_branch, else_branch, .. } => {
            desugar_loop_in_expr(expr);
            desugar_loop_in_block(then_branch);
            if let Some(eb) = else_branch {
                desugar_loop_in_expr(eb);
            }
        }
        ExprKind::While { cond, body } => {
            desugar_loop_in_expr(cond);
            desugar_loop_in_block(body);
        }
        ExprKind::ForIn { iter, body, .. } => {
            desugar_loop_in_expr(iter);
            desugar_loop_in_block(body);
        }
        ExprKind::Match { scrutinee, arms } => {
            desugar_loop_in_expr(scrutinee);
            for a in arms.iter_mut() {
                desugar_loop_in_expr(&mut a.body);
            }
        }
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
            desugar_loop_in_expr(lhs);
            desugar_loop_in_expr(rhs);
        }
        ExprKind::Unary { expr, .. }
        | ExprKind::Cast { expr, .. }
        | ExprKind::TypeTest { expr, .. }
        | ExprKind::TypeDowncast { expr, .. } => desugar_loop_in_expr(expr),
        ExprKind::Some(inner) | ExprKind::Await(inner) => desugar_loop_in_expr(inner),
        ExprKind::Return(opt) | ExprKind::Break(opt) => {
            if let Some(inner) = opt {
                desugar_loop_in_expr(inner);
            }
        }
        ExprKind::Assign { value, .. } => desugar_loop_in_expr(value),
        ExprKind::AssignField { obj, value, .. } => {
            desugar_loop_in_expr(obj);
            desugar_loop_in_expr(value);
        }
        ExprKind::AssignIndex { obj, index, value } => {
            desugar_loop_in_expr(obj);
            desugar_loop_in_expr(index);
            desugar_loop_in_expr(value);
        }
        ExprKind::Call { args, .. }
        | ExprKind::SuperCall { args, .. }
        | ExprKind::New { args, .. } => {
            for a in args.iter_mut() {
                desugar_loop_in_expr(a);
            }
        }
        ExprKind::MethodCall { obj, args, .. } => {
            desugar_loop_in_expr(obj);
            for a in args.iter_mut() {
                desugar_loop_in_expr(a);
            }
        }
        ExprKind::Field { obj, .. } => desugar_loop_in_expr(obj),
        ExprKind::Index { obj, index } => {
            desugar_loop_in_expr(obj);
            desugar_loop_in_expr(index);
        }
        ExprKind::Tuple(es) | ExprKind::Array(es) => {
            for x in es.iter_mut() {
                desugar_loop_in_expr(x);
            }
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start {
                desugar_loop_in_expr(s);
            }
            if let Some(eb) = end {
                desugar_loop_in_expr(eb);
            }
        }
        ExprKind::FnExpr { body, .. } => desugar_loop_in_block(body),
        _ => {}
    }
}

// --- `while await cond` → `while true { let __wcond_N = await cond; if !cond_v { break } ... }` --
//
// `while cond` re-evaluates cond every iteration; awaiting inside
// cond should keep that property. We rewrite to a `while true`
// loop whose body starts with `let __wcond_N = <orig cond>` (the
// lift_subexpr_awaits pass that runs AFTER this one will canonicalize
// any sub-expression awaits inside `<orig cond>` into their own let
// statements) followed by `if !__wcond_N { break }`.

pub fn desugar_while_cond_await(mut body: Block) -> Block {
    let mut counter: u64 = 0;
    desugar_while_cond_in_block(&mut body, &mut counter);
    body
}

fn desugar_while_cond_in_block(b: &mut Block, counter: &mut u64) {
    for s in b.stmts.iter_mut() {
        desugar_while_cond_in_stmt(s, counter);
    }
    if let Some(t) = b.tail.as_mut() {
        desugar_while_cond_in_expr(t, counter);
    }
}

fn desugar_while_cond_in_stmt(s: &mut Stmt, counter: &mut u64) {
    match &mut s.kind {
        StmtKind::Let { value, .. }
        | StmtKind::LetTuple { value, .. }
        | StmtKind::LetStruct { value, .. } => desugar_while_cond_in_expr(value, counter),
        StmtKind::Expr(e) => desugar_while_cond_in_expr(e, counter),
    }
}

fn desugar_while_cond_in_expr(e: &mut Expr, counter: &mut u64) {
    match &mut e.kind {
        ExprKind::While { cond, body } => {
            desugar_while_cond_in_expr(cond, counter);
            desugar_while_cond_in_block(body, counter);
            if expr_has_await(cond) {
                let span = e.span;
                *counter += 1;
                let cv = Symbol::intern(&format!("__wcond_{}", counter));
                // let __wcond_N = <orig cond>
                let let_cv = Stmt::new(
                    StmtKind::Let {
                        is_pub: false,
                        is_const: false,
                        name: cv,
                        ty: None,
                        value: (**cond).clone(),
                    },
                    span,
                );
                // if !__wcond_N { break }
                let break_if = Stmt::new(
                    StmtKind::Expr(Expr::new(
                        ExprKind::If {
                            cond: Box::new(Expr::new(
                                ExprKind::Unary {
                                    op: ilang_ast::UnOp::Not,
                                    expr: Box::new(Expr::new(ExprKind::Var(cv), span)),
                                },
                                span,
                            )),
                            then_branch: Block {
                                stmts: vec![Stmt::new(
                                    StmtKind::Expr(Expr::new(ExprKind::Break(None), span)),
                                    span,
                                )],
                                tail: None,
                            },
                            else_branch: None,
                        },
                        span,
                    )),
                    span,
                );
                let mut new_stmts: Vec<Stmt> = Vec::with_capacity(body.stmts.len() + 2);
                new_stmts.push(let_cv);
                new_stmts.push(break_if);
                for s in std::mem::take(&mut body.stmts) {
                    new_stmts.push(s);
                }
                body.stmts = new_stmts;
                // The body tail (if any) is preserved; iteration
                // discards its value as usual.
                **cond = Expr::new(ExprKind::Bool(true), span);
            }
        }
        ExprKind::Block(b) => desugar_while_cond_in_block(b, counter),
        ExprKind::If { cond, then_branch, else_branch } => {
            desugar_while_cond_in_expr(cond, counter);
            desugar_while_cond_in_block(then_branch, counter);
            if let Some(eb) = else_branch {
                desugar_while_cond_in_expr(eb, counter);
            }
        }
        ExprKind::IfLet { expr, then_branch, else_branch, .. } => {
            desugar_while_cond_in_expr(expr, counter);
            desugar_while_cond_in_block(then_branch, counter);
            if let Some(eb) = else_branch {
                desugar_while_cond_in_expr(eb, counter);
            }
        }
        ExprKind::Loop { body } => desugar_while_cond_in_block(body, counter),
        ExprKind::ForIn { iter, body, .. } => {
            desugar_while_cond_in_expr(iter, counter);
            desugar_while_cond_in_block(body, counter);
        }
        ExprKind::Match { scrutinee, arms } => {
            desugar_while_cond_in_expr(scrutinee, counter);
            for a in arms.iter_mut() {
                desugar_while_cond_in_expr(&mut a.body, counter);
            }
        }
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
            desugar_while_cond_in_expr(lhs, counter);
            desugar_while_cond_in_expr(rhs, counter);
        }
        ExprKind::Unary { expr, .. }
        | ExprKind::Cast { expr, .. }
        | ExprKind::TypeTest { expr, .. }
        | ExprKind::TypeDowncast { expr, .. } => desugar_while_cond_in_expr(expr, counter),
        ExprKind::Some(inner) | ExprKind::Await(inner) => {
            desugar_while_cond_in_expr(inner, counter)
        }
        ExprKind::Return(opt) | ExprKind::Break(opt) => {
            if let Some(inner) = opt {
                desugar_while_cond_in_expr(inner, counter);
            }
        }
        ExprKind::Assign { value, .. } => desugar_while_cond_in_expr(value, counter),
        ExprKind::AssignField { obj, value, .. } => {
            desugar_while_cond_in_expr(obj, counter);
            desugar_while_cond_in_expr(value, counter);
        }
        ExprKind::AssignIndex { obj, index, value } => {
            desugar_while_cond_in_expr(obj, counter);
            desugar_while_cond_in_expr(index, counter);
            desugar_while_cond_in_expr(value, counter);
        }
        ExprKind::Call { args, .. }
        | ExprKind::SuperCall { args, .. }
        | ExprKind::New { args, .. } => {
            for a in args.iter_mut() {
                desugar_while_cond_in_expr(a, counter);
            }
        }
        ExprKind::MethodCall { obj, args, .. } => {
            desugar_while_cond_in_expr(obj, counter);
            for a in args.iter_mut() {
                desugar_while_cond_in_expr(a, counter);
            }
        }
        ExprKind::Field { obj, .. } => desugar_while_cond_in_expr(obj, counter),
        ExprKind::Index { obj, index } => {
            desugar_while_cond_in_expr(obj, counter);
            desugar_while_cond_in_expr(index, counter);
        }
        ExprKind::Tuple(es) | ExprKind::Array(es) => {
            for x in es.iter_mut() {
                desugar_while_cond_in_expr(x, counter);
            }
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start {
                desugar_while_cond_in_expr(s, counter);
            }
            if let Some(eb) = end {
                desugar_while_cond_in_expr(eb, counter);
            }
        }
        ExprKind::FnExpr { body, .. } => desugar_while_cond_in_block(body, counter),
        _ => {}
    }
}

// --- for-in → while pre-desugar -----------------------------------
//
// `for v in start..end { body }` (with awaits inside body) is
// rewritten to:
//
//   { let __for_end_N = end
//     let v = start
//     while v < __for_end_N {        // `<=` if inclusive
//         body
//         v = v + 1
//     } }
//
// Only `Range { start: Some(_), end: Some(_) }` forms are
// supported. Other iter shapes (arrays, half-open ranges, custom
// iterators) are left in place — body_is_supported will then
// reject them and the dispatcher surfaces an error.

/// Walk a block tree and replace every `for v in s..e { body }` with
/// awaits in `body` by its while-desugared equivalent. Recurses into
/// every block/expr.
pub fn desugar_for_in_with_await(mut body: Block) -> Block {
    let mut counter: u64 = 0;
    desugar_for_in_in_block(&mut body, &mut counter);
    body
}

fn desugar_for_in_in_block(b: &mut Block, counter: &mut u64) {
    let mut new_stmts: Vec<Stmt> = Vec::with_capacity(b.stmts.len());
    for s in std::mem::take(&mut b.stmts) {
        let mut s = s;
        desugar_for_in_in_stmt(&mut s, counter);
        // If the stmt is now an Expr-stmt whose expr is a Block
        // (from the desugar), flatten its stmts into the outer
        // block to keep variable scope reasonable.
        if let StmtKind::Expr(e) = &s.kind {
            if let ExprKind::Block(inner) = &e.kind {
                // Flatten only when the inner block came from a
                // for-in desugar (no tail value).
                if inner.tail.is_none() {
                    for is_ in inner.stmts.clone() {
                        new_stmts.push(is_);
                    }
                    continue;
                }
            }
        }
        new_stmts.push(s);
    }
    b.stmts = new_stmts;
    if let Some(t) = b.tail.as_mut() {
        desugar_for_in_in_expr(t, counter);
    }
}

fn desugar_for_in_in_stmt(s: &mut Stmt, counter: &mut u64) {
    match &mut s.kind {
        StmtKind::Let { value, .. }
        | StmtKind::LetTuple { value, .. }
        | StmtKind::LetStruct { value, .. } => desugar_for_in_in_expr(value, counter),
        StmtKind::Expr(e) => desugar_for_in_in_expr(e, counter),
    }
}

fn desugar_for_in_in_expr(e: &mut Expr, counter: &mut u64) {
    // Recurse into sub-trees first so inner for-ins are desugared
    // before we (possibly) rewrite this node.
    match &mut e.kind {
        ExprKind::Block(b) => desugar_for_in_in_block(b, counter),
        ExprKind::If { cond, then_branch, else_branch } => {
            desugar_for_in_in_expr(cond, counter);
            desugar_for_in_in_block(then_branch, counter);
            if let Some(eb) = else_branch {
                desugar_for_in_in_expr(eb, counter);
            }
        }
        ExprKind::IfLet { expr, then_branch, else_branch, .. } => {
            desugar_for_in_in_expr(expr, counter);
            desugar_for_in_in_block(then_branch, counter);
            if let Some(eb) = else_branch {
                desugar_for_in_in_expr(eb, counter);
            }
        }
        ExprKind::While { cond, body } => {
            desugar_for_in_in_expr(cond, counter);
            desugar_for_in_in_block(body, counter);
        }
        ExprKind::Loop { body } => desugar_for_in_in_block(body, counter),
        ExprKind::Match { scrutinee, arms } => {
            desugar_for_in_in_expr(scrutinee, counter);
            for a in arms.iter_mut() {
                desugar_for_in_in_expr(&mut a.body, counter);
            }
        }
        ExprKind::ForIn { var, iter, body } => {
            // Recurse into the body first (handles nested for-in).
            desugar_for_in_in_block(body, counter);
            desugar_for_in_in_expr(iter, counter);
            // Only rewrite if the loop body contains awaits. Sync
            // for-in is handled downstream as-is.
            if !block_has_await(body) {
                return;
            }
            let span = e.span;
            let var_name = *var;
            match &iter.kind {
                ExprKind::Range { start, end, inclusive } => {
                    let start_opt = start.as_deref().cloned();
                    let end_opt = end.as_deref().cloned();
                    let inclusive = *inclusive;
                    let Some(start_expr) = start_opt else {
                        return;
                    };
                    let mut new_body_stmts: Vec<Stmt> = body.stmts.clone();
                    if let Some(t) = body.tail.as_ref() {
                        new_body_stmts.push(Stmt::new(StmtKind::Expr((**t).clone()), span));
                    }
                    let inc = Expr::new(
                        ExprKind::Assign {
                            target: var_name,
                            value: Box::new(Expr::new(
                                ExprKind::Binary {
                                    op: ilang_ast::BinOp::Add,
                                    lhs: Box::new(Expr::new(ExprKind::Var(var_name), span)),
                                    rhs: Box::new(Expr::new(ExprKind::Int(1), span)),
                                },
                                span,
                            )),
                        },
                        span,
                    );
                    new_body_stmts.push(Stmt::new(StmtKind::Expr(inc), span));
                    let new_body = Block { stmts: new_body_stmts, tail: None };
                    let mut stmts: Vec<Stmt> = Vec::with_capacity(3);
                    let cond = match end_opt {
                        Some(end_expr) => {
                            *counter += 1;
                            let end_name = Symbol::intern(&format!("__for_end_{}", counter));
                            stmts.push(Stmt::new(
                                StmtKind::Let {
                                    is_pub: false,
                                    is_const: false,
                                    name: end_name,
                                    ty: None,
                                    value: end_expr,
                                },
                                span,
                            ));
                            let cmp_op = if inclusive {
                                ilang_ast::BinOp::Le
                            } else {
                                ilang_ast::BinOp::Lt
                            };
                            Expr::new(
                                ExprKind::Binary {
                                    op: cmp_op,
                                    lhs: Box::new(Expr::new(ExprKind::Var(var_name), span)),
                                    rhs: Box::new(Expr::new(ExprKind::Var(end_name), span)),
                                },
                                span,
                            )
                        }
                        None => Expr::new(ExprKind::Bool(true), span),
                    };
                    let while_expr = Expr::new(
                        ExprKind::While {
                            cond: Box::new(cond),
                            body: new_body,
                        },
                        span,
                    );
                    stmts.push(Stmt::new(
                        StmtKind::Let {
                            is_pub: false,
                            is_const: false,
                            name: var_name,
                            ty: None,
                            value: start_expr,
                        },
                        span,
                    ));
                    stmts.push(Stmt::new(StmtKind::Expr(while_expr), span));
                    *e = Expr::new(
                        ExprKind::Block(Block { stmts, tail: None }),
                        span,
                    );
                }
                _ => {
                    // Array-like iter: index-based while loop.
                    //   { let __arr_N = iter
                    //     let __i_N = 0
                    //     while __i_N < __arr_N.length {
                    //         let var = __arr_N[__i_N]
                    //         body
                    //         __i_N = __i_N + 1
                    //     } }
                    *counter += 1;
                    let arr_name = Symbol::intern(&format!("__for_arr_{}", counter));
                    let idx_name = Symbol::intern(&format!("__for_i_{}", counter));
                    let iter_expr = (**iter).clone();
                    let arr_var = || Expr::new(ExprKind::Var(arr_name), span);
                    let idx_var = || Expr::new(ExprKind::Var(idx_name), span);
                    // let var = __arr_N[__i_N]
                    let elem_let = Stmt::new(
                        StmtKind::Let {
                            is_pub: false,
                            is_const: false,
                            name: var_name,
                            ty: None,
                            value: Expr::new(
                                ExprKind::Index {
                                    obj: Box::new(arr_var()),
                                    index: Box::new(idx_var()),
                                },
                                span,
                            ),
                        },
                        span,
                    );
                    let mut new_body_stmts: Vec<Stmt> = Vec::with_capacity(body.stmts.len() + 3);
                    new_body_stmts.push(elem_let);
                    for s in body.stmts.iter() {
                        new_body_stmts.push(s.clone());
                    }
                    if let Some(t) = body.tail.as_ref() {
                        new_body_stmts.push(Stmt::new(StmtKind::Expr((**t).clone()), span));
                    }
                    let inc = Expr::new(
                        ExprKind::Assign {
                            target: idx_name,
                            value: Box::new(Expr::new(
                                ExprKind::Binary {
                                    op: ilang_ast::BinOp::Add,
                                    lhs: Box::new(idx_var()),
                                    rhs: Box::new(Expr::new(ExprKind::Int(1), span)),
                                },
                                span,
                            )),
                        },
                        span,
                    );
                    new_body_stmts.push(Stmt::new(StmtKind::Expr(inc), span));
                    let new_body = Block { stmts: new_body_stmts, tail: None };
                    let cond = Expr::new(
                        ExprKind::Binary {
                            op: ilang_ast::BinOp::Lt,
                            lhs: Box::new(idx_var()),
                            rhs: Box::new(Expr::new(
                                ExprKind::Field {
                                    obj: Box::new(arr_var()),
                                    name: Symbol::intern("length"),
                                },
                                span,
                            )),
                        },
                        span,
                    );
                    let while_expr = Expr::new(
                        ExprKind::While {
                            cond: Box::new(cond),
                            body: new_body,
                        },
                        span,
                    );
                    let stmts = vec![
                        Stmt::new(
                            StmtKind::Let {
                                is_pub: false,
                                is_const: false,
                                name: arr_name,
                                ty: None,
                                value: iter_expr,
                            },
                            span,
                        ),
                        Stmt::new(
                            StmtKind::Let {
                                is_pub: false,
                                is_const: false,
                                name: idx_name,
                                ty: None,
                                value: Expr::new(ExprKind::Int(0), span),
                            },
                            span,
                        ),
                        Stmt::new(StmtKind::Expr(while_expr), span),
                    ];
                    *e = Expr::new(
                        ExprKind::Block(Block { stmts, tail: None }),
                        span,
                    );
                }
            }
        }
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
            desugar_for_in_in_expr(lhs, counter);
            desugar_for_in_in_expr(rhs, counter);
        }
        ExprKind::Unary { expr, .. }
        | ExprKind::Cast { expr, .. }
        | ExprKind::TypeTest { expr, .. }
        | ExprKind::TypeDowncast { expr, .. } => desugar_for_in_in_expr(expr, counter),
        ExprKind::Some(inner) | ExprKind::Await(inner) => desugar_for_in_in_expr(inner, counter),
        ExprKind::Return(opt) | ExprKind::Break(opt) => {
            if let Some(inner) = opt {
                desugar_for_in_in_expr(inner, counter);
            }
        }
        ExprKind::Assign { value, .. } => desugar_for_in_in_expr(value, counter),
        ExprKind::AssignField { obj, value, .. } => {
            desugar_for_in_in_expr(obj, counter);
            desugar_for_in_in_expr(value, counter);
        }
        ExprKind::AssignIndex { obj, index, value } => {
            desugar_for_in_in_expr(obj, counter);
            desugar_for_in_in_expr(index, counter);
            desugar_for_in_in_expr(value, counter);
        }
        ExprKind::Call { args, .. }
        | ExprKind::SuperCall { args, .. }
        | ExprKind::New { args, .. } => {
            for a in args.iter_mut() {
                desugar_for_in_in_expr(a, counter);
            }
        }
        ExprKind::MethodCall { obj, args, .. } => {
            desugar_for_in_in_expr(obj, counter);
            for a in args.iter_mut() {
                desugar_for_in_in_expr(a, counter);
            }
        }
        ExprKind::Field { obj, .. } => desugar_for_in_in_expr(obj, counter),
        ExprKind::Index { obj, index } => {
            desugar_for_in_in_expr(obj, counter);
            desugar_for_in_in_expr(index, counter);
        }
        ExprKind::Tuple(es) | ExprKind::Array(es) => {
            for x in es.iter_mut() {
                desugar_for_in_in_expr(x, counter);
            }
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start {
                desugar_for_in_in_expr(s, counter);
            }
            if let Some(eb) = end {
                desugar_for_in_in_expr(eb, counter);
            }
        }
        ExprKind::FnExpr { body, .. } => desugar_for_in_in_block(body, counter),
        _ => {}
    }
}

/// Try to resolve the static type of a Var expression by looking
/// it up in the cumulative live-set (which contains params + let
/// bindings introduced upstream). Returns None for non-Var or
/// unknown names — caller falls back to the I64 placeholder.
fn resolve_var_ty(e: &Expr, cumulative_fields: &[(Symbol, Type)]) -> Option<Type> {
    if let ExprKind::Var(name) = &e.kind {
        for (n, t) in cumulative_fields {
            if n == name {
                return Some(t.clone());
            }
        }
    }
    None
}

/// Substitute every `Type::TypeVar(name)` in `t` with its mapped
/// concrete type from `subst`. Used when resolving variant payload
/// types of a generic enum instantiation (e.g. `Box<i64>` mapping
/// `T → i64` over a variant's `T` payload).
fn substitute_type(t: &Type, subst: &HashMap<Symbol, Type>) -> Type {
    match t {
        Type::TypeVar(n) => subst.get(n).cloned().unwrap_or_else(|| t.clone()),
        // The parser can't distinguish a type-parameter reference from
        // a class name at parse time — it emits `Type::Object(name)`
        // for both. If `name` is in the substitution map, treat it
        // as a type-param reference and replace.
        Type::Object(n) if subst.contains_key(n) => subst.get(n).cloned().unwrap(),
        Type::Optional(inner) => Type::Optional(Box::new(substitute_type(inner, subst))),
        Type::Weak(inner) => Type::Weak(Box::new(substitute_type(inner, subst))),
        Type::Array { elem, fixed } => Type::Array {
            elem: Box::new(substitute_type(elem, subst)),
            fixed: *fixed,
        },
        Type::Tuple(ts) => Type::Tuple(
            ts.iter().map(|t| substitute_type(t, subst)).collect::<Vec<_>>().into_boxed_slice(),
        ),
        Type::Generic(g) => Type::generic(
            g.base,
            g.args.iter().map(|a| substitute_type(a, subst)).collect(),
        ),
        Type::Fn(f) => Type::func(
            f.params.iter().map(|p| substitute_type(p, subst)).collect(),
            substitute_type(&f.ret, subst),
        ),
        _ => t.clone(),
    }
}

/// Compute (binding_name, binding_type) pairs introduced by `pattern`
/// when matched against a value of `scrutinee_ty`. Built-in
/// `Optional<T>` / `Result<T,E>` / user enums (including generic
/// ones via type_param substitution) are resolved precisely;
/// unknown shapes yield `Type::I64` placeholders for any bindings.
fn pattern_binding_types(
    pattern: &Pattern,
    scrutinee_ty: Option<&Type>,
    enums: &HashMap<Symbol, EnumDecl>,
) -> Vec<(Symbol, Type)> {
    let PatternKind::Variant { variant, bindings, .. } = &pattern.kind else {
        return Vec::new();
    };
    let mut payload_tys: Option<Vec<Type>> = None;
    let mut payload_struct: HashMap<Symbol, Type> = HashMap::new();
    // Resolve user-enum payload using an enum decl + type-arg
    // substitution.
    let mut resolve_user_enum = |ed: &EnumDecl, subst: HashMap<Symbol, Type>| {
        if let Some(v) = ed.variants.iter().find(|v| v.name == *variant) {
            match &v.payload {
                VariantPayload::Unit => payload_tys = Some(Vec::new()),
                VariantPayload::Tuple(tys) => {
                    payload_tys =
                        Some(tys.iter().map(|t| substitute_type(t, &subst)).collect());
                }
                VariantPayload::Struct(fs) => {
                    for f in fs.iter() {
                        payload_struct.insert(f.name, substitute_type(&f.ty, &subst));
                    }
                }
            }
        }
    };
    match scrutinee_ty {
        Some(Type::Optional(inner)) => match variant.as_str() {
            "some" => payload_tys = Some(vec![(**inner).clone()]),
            "none" => payload_tys = Some(Vec::new()),
            _ => {}
        },
        Some(Type::Generic(g)) if g.base.as_str() == "Result" => {
            match (variant.as_str(), g.args.get(0), g.args.get(1)) {
                ("ok", Some(t), _) => payload_tys = Some(vec![t.clone()]),
                ("err", _, Some(e)) => payload_tys = Some(vec![e.clone()]),
                _ => {}
            }
        }
        Some(Type::Generic(g)) => {
            // User-defined generic enum: `EnumName<A, B>`. Build a
            // type_param → arg substitution and resolve.
            if let Some(ed) = enums.get(&g.base) {
                let subst: HashMap<Symbol, Type> = ed
                    .type_params
                    .iter()
                    .zip(g.args.iter())
                    .map(|(p, a)| (*p, a.clone()))
                    .collect();
                resolve_user_enum(ed, subst);
            }
        }
        Some(Type::Enum(name)) | Some(Type::Object(name)) => {
            if let Some(ed) = enums.get(name) {
                resolve_user_enum(ed, HashMap::new());
            }
        }
        _ => {}
    }
    match bindings {
        PatternBindings::Unit => Vec::new(),
        PatternBindings::Tuple(names) => names
            .iter()
            .enumerate()
            .filter(|(_, n)| n.as_str() != "_")
            .map(|(i, n)| {
                let t = payload_tys
                    .as_ref()
                    .and_then(|ts| ts.get(i).cloned())
                    .unwrap_or(Type::I64);
                (*n, t)
            })
            .collect(),
        PatternBindings::Struct(pairs) => pairs
            .iter()
            .filter(|(_, b)| b.as_str() != "_")
            .map(|(field, bind)| {
                let t = payload_struct.get(field).cloned().unwrap_or(Type::I64);
                (*bind, t)
            })
            .collect(),
    }
}

// --- AST construction helpers ---------------------------------------

fn mk_var(name: Symbol, span: Span) -> Expr {
    Expr::new(ExprKind::Var(name), span)
}
fn mk_int(n: i64, span: Span) -> Expr {
    Expr::new(ExprKind::Int(n), span)
}
fn mk_field(obj: Expr, name: Symbol, span: Span) -> Expr {
    Expr::new(ExprKind::Field { obj: Box::new(obj), name }, span)
}
fn mk_assign_field(obj: Expr, field: Symbol, value: Expr, span: Span) -> Expr {
    Expr::new(
        ExprKind::AssignField {
            obj: Box::new(obj),
            field,
            value: Box::new(value),
            is_init: false,
        },
        span,
    )
}
fn mk_method_call(obj: Expr, method: Symbol, args: Vec<Expr>, span: Span) -> Expr {
    Expr::new(
        ExprKind::MethodCall {
            obj: Box::new(obj),
            method,
            args: args.into_boxed_slice(),
        },
        span,
    )
}
fn mk_call(callee: Symbol, args: Vec<Expr>, span: Span) -> Expr {
    Expr::new(
        ExprKind::Call { callee, args: args.into_boxed_slice() },
        span,
    )
}
fn mk_let(name: Symbol, ty: Option<Type>, value: Expr, span: Span) -> Stmt {
    Stmt::new(
        StmtKind::Let { is_pub: false, is_const: false, name, ty, value },
        span,
    )
}
fn mk_expr_stmt(e: Expr, span: Span) -> Stmt {
    Stmt::new(StmtKind::Expr(e), span)
}
fn mk_enum_ctor_struct(
    enum_name: Symbol,
    variant: Symbol,
    fields: Vec<(Symbol, Expr)>,
    span: Span,
) -> Expr {
    Expr::new(
        ExprKind::EnumCtor {
            enum_name,
            variant,
            args: CtorArgs::Struct(fields.into_boxed_slice()),
        },
        span,
    )
}

// --- Segment data structures ----------------------------------------

/// One segment of an async fn body — code between two awaits (or
/// between the body start and the first await, or between the last
/// await and the body end). Identified by `idx` (0-based), which
/// corresponds to the state-enum variant `S{idx}`.
#[derive(Debug, Clone)]
pub struct Segment {
    pub idx: u32,
    /// Field layout for the variant payload that represents this
    /// segment. Over-approximate (params + every let-binding
    /// introduced before this segment, in source order). Always
    /// includes `__this` for class-method asyncs.
    pub fields: Vec<(Symbol, Type)>,
    /// Sync stmts to execute when this variant is matched.
    pub stmts: Vec<Stmt>,
    /// Terminator action.
    pub terminator: SegTerm,
    /// If this segment lives inside a `while` body, the (header_idx,
    /// after_idx) of the enclosing loop. Used by emit_segment_arm
    /// to rewrite `break` → transition-to-after and `continue` →
    /// transition-to-header within the segment's stmts.
    pub loop_info: Option<(u32, u32)>,
}

#[derive(Debug, Clone)]
pub enum SegTerm {
    /// `let <binding>: <binding_ty> = await <promise>` — schedule
    /// `promise` with a continuation that builds variant
    /// `S{next_idx}` from the destructured-and-new locals.
    Suspend {
        promise: Expr,
        binding: Symbol,
        binding_ty: Type,
        next_idx: u32,
    },
    /// Tail-position `if cond { ... } else { ... }`. Each branch
    /// has its own segment chain (rooted at `then_idx` / `else_idx`)
    /// that independently settles the result promise. There is no
    /// post-join segment (Phase 2a doesn't support a `let r = if
    /// ... { ... } ...rest` shape).
    Branch {
        cond: Expr,
        then_idx: u32,
        else_idx: u32,
    },
    /// Unconditional transition to `target_idx`. Used as the
    /// "back edge" of a `while` body (back to the header) and as
    /// the "fall-through" from before a `while` into the header.
    Jump { target_idx: u32 },
    /// Tail-position `match scrutinee { arm1 => target1, ... }`.
    /// Each arm picks a target segment to continue at; pattern
    /// bindings introduced by the arm become locals of the target
    /// variant's payload.
    MatchT {
        scrutinee: Expr,
        arms: Vec<MatchTArm>,
    },
    /// Mid-body join: transition to `target_idx` after binding
    /// `binding = value` into the destination variant's payload.
    /// Used to merge per-arm results of a `let r = if-else/match`
    /// back into the post-construct segment chain.
    JumpBind {
        target_idx: u32,
        binding: Symbol,
        value: Expr,
    },
    /// Final segment: settle the result promise with `value`.
    Settle { value: Expr },
}

#[derive(Debug, Clone)]
pub struct MatchTArm {
    pub pattern: Pattern,
    pub target_idx: u32,
}

/// Output of `lower_async_fn_v2` for an await-containing body.
pub struct StateMachineOutput {
    pub wrapper: FnDecl,
    pub state_enum: EnumDecl,
    pub state_ref_class: ClassDecl,
    pub poll_fn: FnDecl,
}

pub enum LowerOutput {
    /// Body had no awaits — caller falls back to the trivial
    /// `Promise.resolve(...)` wrap (not handled here).
    NoAwait,
    /// Body contained control flow (`if` / `while` / `match`) that
    /// Phase 1's straight-line builder doesn't support. Caller
    /// falls back to the legacy class-based lowering.
    NeedsFallback,
    /// Built the enum-variant state machine.
    Built(StateMachineOutput),
}

// --- Body shape detection -------------------------------------------

fn body_is_straight_line(body: &Block) -> bool {
    // Straight-line means: every body stmt is either a sync let /
    // expr stmt without nested control flow involving awaits, OR a
    // let-await. The body tail is any expr that doesn't itself
    // contain an unlifted await (the lift pass handles sub-exprs).
    for s in &body.stmts {
        if !stmt_is_straight_line(s) {
            return false;
        }
    }
    if let Some(t) = &body.tail {
        if expr_contains_control_flow_with_await(t) {
            return false;
        }
    }
    true
}

/// Body shape that Phase 2a's builder supports — straight-line, OR
/// straight-line stmts followed by a tail-position if-else whose
/// branches are themselves Phase-2a bodies. Cond must have no
/// unlifted await (the lift pass already moves them out).
fn body_is_supported(body: &Block) -> bool {
    for s in &body.stmts {
        if !stmt_is_supported_for_body(s) {
            return false;
        }
    }
    let Some(t) = body.tail.as_deref() else {
        return true;
    };
    if let ExprKind::If { cond, then_branch, else_branch } = &t.kind {
        let has_await_in_branches = block_has_await(then_branch)
            || else_branch.as_deref().is_some_and(expr_has_await);
        if has_await_in_branches {
            // Tail-If with awaits: needs Branch terminator. Requires
            // else (which may be a Block OR an `if` expr — else-if
            // chains coerce into a Block whose tail is the chained If).
            if expr_has_await(cond) {
                return false;
            }
            let Some(eb) = else_branch.as_deref() else {
                return false;
            };
            let else_block = coerce_to_block(eb);
            return body_is_supported(then_branch) && body_is_supported(&else_block);
        }
        // Sync tail-If (no awaits in branches) — fall through to the
        // generic check below.
    }
    if let ExprKind::Match { scrutinee, arms } = &t.kind {
        let has_arm_await = arms.iter().any(|a| expr_has_await(&a.body));
        if has_arm_await {
            if expr_has_await(scrutinee) {
                return false;
            }
            for a in arms.iter() {
                let arm_block = match &a.body.kind {
                    ExprKind::Block(b) => b.clone(),
                    _ => Block {
                        stmts: Vec::new(),
                        tail: Some(Box::new(a.body.clone())),
                    },
                };
                if !body_is_supported(&arm_block) {
                    return false;
                }
            }
            return true;
        }
        // Sync tail-Match — fall through to generic check.
    }
    !expr_contains_control_flow_with_await(t)
}

fn stmt_is_straight_line(s: &Stmt) -> bool {
    match &s.kind {
        StmtKind::Let { value, .. } => !expr_contains_control_flow_with_await(value),
        StmtKind::LetTuple { value, .. } | StmtKind::LetStruct { value, .. } => {
            !expr_contains_control_flow_with_await(value)
        }
        StmtKind::Expr(e) => match &e.kind {
            // While/Loop/For as statements don't qualify for the
            // straight-line case — but `body_is_supported` adds a
            // separate clause for them.
            ExprKind::While { .. } | ExprKind::Loop { .. } | ExprKind::ForIn { .. } => {
                !block_or_expr_has_await_inside(e)
            }
            _ => !expr_contains_control_flow_with_await(e),
        },
    }
}

/// Phase 2b/2d: a stmt that's either straight-line OR a supported
/// while-with-await OR a `let X = if-else/match` whose branches /
/// arms have awaits (mid-body join).
fn stmt_is_supported_for_body(s: &Stmt) -> bool {
    if stmt_is_straight_line(s) {
        return true;
    }
    if let StmtKind::Expr(e) = &s.kind {
        if let ExprKind::While { cond, body } = &e.kind {
            if expr_has_await(cond) {
                return false;
            }
            return loop_body_is_supported(body);
        }
        // Stmt-position `if cond { ...await... } [else { ... }]` —
        // both arms flow to a synthesized after segment. else may
        // be omitted (then we just synthesize an empty else→after
        // jump).
        if let ExprKind::If { cond, then_branch, else_branch } = &e.kind {
            let any_await = block_has_await(then_branch)
                || else_branch.as_deref().is_some_and(expr_has_await);
            if any_await {
                if expr_has_await(cond) {
                    return false;
                }
                if !body_is_supported(then_branch) {
                    return false;
                }
                if let Some(eb) = else_branch.as_deref() {
                    let else_blk = coerce_to_block(eb);
                    if !body_is_supported(&else_blk) {
                        return false;
                    }
                }
                return true;
            }
        }
    }
    if let StmtKind::Let { value, .. } = &s.kind {
        // mid-body `let X = if-else { ...await... }` (Phase 2d join).
        if let ExprKind::If { cond, then_branch, else_branch } = &value.kind {
            if expr_has_await(cond) {
                return false;
            }
            let Some(eb) = else_branch.as_deref() else {
                return false;
            };
            let else_block = coerce_to_block(eb);
            return body_is_supported(then_branch) && body_is_supported(&else_block);
        }
        // mid-body `let X = match { ...await... }` (Phase 2d join).
        if let ExprKind::Match { scrutinee, arms } = &value.kind {
            if expr_has_await(scrutinee) {
                return false;
            }
            for a in arms.iter() {
                let arm_block = match &a.body.kind {
                    ExprKind::Block(b) => b.clone(),
                    _ => Block {
                        stmts: Vec::new(),
                        tail: Some(Box::new(a.body.clone())),
                    },
                };
                if !body_is_supported(&arm_block) {
                    return false;
                }
            }
            return true;
        }
    }
    false
}

/// Loop body shape: Phase 2e unifies the check with the regular
/// `body_is_supported`. A loop body can now contain anything the
/// builder handles in a fn body — tail-If/match with awaits,
/// mid-body let-if/match join, nested while.
fn loop_body_is_supported(body: &Block) -> bool {
    body_is_supported(body)
}

fn block_or_expr_has_await_inside(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::While { body, .. } | ExprKind::Loop { body, .. } => {
            block_has_await(body)
        }
        ExprKind::ForIn { body, .. } => block_has_await(body),
        _ => false,
    }
}

fn block_has_await(b: &Block) -> bool {
    for s in &b.stmts {
        if stmt_has_await(s) {
            return true;
        }
    }
    if let Some(t) = &b.tail {
        if expr_has_await(t) {
            return true;
        }
    }
    false
}

fn stmt_has_await(s: &Stmt) -> bool {
    match &s.kind {
        StmtKind::Let { value, .. } => expr_has_await(value),
        StmtKind::LetTuple { value, .. } | StmtKind::LetStruct { value, .. } => {
            expr_has_await(value)
        }
        StmtKind::Expr(e) => expr_has_await(e),
    }
}

fn expr_has_await(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Await(_) => true,
        ExprKind::If { cond, then_branch, else_branch } => {
            expr_has_await(cond)
                || block_has_await(then_branch)
                || else_branch.as_deref().is_some_and(expr_has_await)
        }
        ExprKind::While { cond, body } => expr_has_await(cond) || block_has_await(body),
        ExprKind::Match { scrutinee, arms } => {
            expr_has_await(scrutinee)
                || arms.iter().any(|a| expr_has_await(&a.body))
        }
        ExprKind::Block(b) => block_has_await(b),
        _ => false,
    }
}

fn expr_contains_control_flow_with_await(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::If { then_branch, else_branch, .. } => {
            block_has_await(then_branch)
                || else_branch.as_deref().is_some_and(expr_has_await)
        }
        ExprKind::While { body, .. } | ExprKind::Loop { body, .. } => {
            block_has_await(body)
        }
        ExprKind::ForIn { body, .. } => block_has_await(body),
        ExprKind::Match { arms, .. } => {
            arms.iter().any(|a| expr_has_await(&a.body))
        }
        _ => false,
    }
}

// --- Segment construction -------------------------------------------

/// What terminator caps the final segment of a block walked by
/// `build_block`. `SettleTail` produces `Settle{value: tail}` from
/// the block's tail expression (the default for fn-body tails and
/// for if-else arms). `JumpTo(idx)` ignores the tail value and
/// unconditionally jumps — used for while-body tails to flow the
/// back-edge into the loop header.
#[derive(Debug, Clone, Copy)]
enum FinalTerm {
    SettleTail,
    JumpTo(u32),
    /// Bind the block's tail value into `binding` of variant
    /// `target_idx`'s payload, then jump. Used for arms that feed a
    /// mid-body join (e.g. `let r = if-else { ...await... }`).
    JumpBindTail { target_idx: u32, binding: Symbol },
}

/// Stateful builder used while walking a block tree.
struct SegBuilder<'a> {
    segments: Vec<Segment>,
    next_idx: u32,
    let_ty: &'a HashMap<Symbol, Type>,
    span: Span,
    /// Innermost enclosing loop's (header_idx, after_idx). Newly
    /// pushed segments inherit this as their `loop_info` so
    /// emit_segment_arm knows where break/continue should jump.
    cur_loop: Option<(u32, u32)>,
    /// Enum declarations in scope, used to resolve match-pattern
    /// binding types from a variant's payload spec. Keyed by enum
    /// name. Built from `Item::Enum` entries by the caller.
    enums: &'a HashMap<Symbol, EnumDecl>,
}

impl<'a> SegBuilder<'a> {
    fn alloc_idx(&mut self) -> u32 {
        let i = self.next_idx;
        self.next_idx += 1;
        i
    }

    /// Push a segment, attaching the current `loop_info` so the
    /// arm-emission pass can resolve break/continue inside its
    /// stmts.
    fn push_seg(&mut self, idx: u32, fields: Vec<(Symbol, Type)>, stmts: Vec<Stmt>, term: SegTerm) {
        self.segments.push(Segment {
            idx,
            fields,
            stmts,
            terminator: term,
            loop_info: self.cur_loop,
        });
    }

    /// Build segments for `block`. On entry, `self_idx` is the
    /// variant index reserved for the FIRST segment this call will
    /// push (allocated by the caller). `cumulative_fields` is the
    /// live-in set for that first segment. `final_term` decides how
    /// the last segment (the one holding the block's tail) is capped.
    fn build_block(
        &mut self,
        block: &Block,
        self_idx: u32,
        mut cumulative_fields: Vec<(Symbol, Type)>,
        final_term: FinalTerm,
    ) {
        let mut cur_stmts: Vec<Stmt> = Vec::new();
        let mut pending_lets: Vec<(Symbol, Type)> = Vec::new();
        let mut idx = self_idx;
        let stmts_slice = &block.stmts[..];
        let mut stmt_i = 0usize;
        while stmt_i < stmts_slice.len() {
            let s = &stmts_slice[stmt_i];
            stmt_i += 1;
            // Mid-body `let X = if-else { ...await... }` / `let X =
            // match { ...await... }`: branch into per-arm chains
            // that converge on a join segment carrying `X` as a new
            // live local. The rest of the outer block is then
            // processed FROM the join idx via a tail-recursive call.
            if let StmtKind::Let { name, value, ty, .. } = &s.kind {
                let join_kind = mid_body_join_kind(value);
                if join_kind.is_some() {
                    let r_ty = ty
                        .clone()
                        .or_else(|| self.let_ty.get(name).cloned())
                        .unwrap_or(Type::I64);
                    let join_idx = self.alloc_idx();
                    let mut branch_live = cumulative_fields.clone();
                    branch_live.append(&mut pending_lets);
                    // Build & push the pre-join (current) segment
                    // with a Branch / MatchT terminator, plus the
                    // per-arm chains.
                    match &value.kind {
                        ExprKind::If { cond, then_branch, else_branch } => {
                            let then_idx = self.alloc_idx();
                            let else_idx = self.alloc_idx();
                            self.push_seg(
                                idx,
                                cumulative_fields,
                                std::mem::take(&mut cur_stmts),
                                SegTerm::Branch {
                                    cond: (**cond).clone(),
                                    then_idx,
                                    else_idx,
                                },
                            );
                            self.build_block(
                                then_branch,
                                then_idx,
                                branch_live.clone(),
                                FinalTerm::JumpBindTail { target_idx: join_idx, binding: *name },
                            );
                            if let Some(eb) = else_branch.as_deref() {
                                let else_blk = coerce_to_block(eb);
                                self.build_block(
                                    &else_blk,
                                    else_idx,
                                    branch_live.clone(),
                                    FinalTerm::JumpBindTail { target_idx: join_idx, binding: *name },
                                );
                            }
                        }
                        ExprKind::Match { scrutinee, arms } => {
                            let scrut_ty = resolve_var_ty(scrutinee, &branch_live);
                            let mut term_arms: Vec<MatchTArm> = Vec::new();
                            let mut per_arm: Vec<(u32, Vec<(Symbol, Type)>, Block)> = Vec::new();
                            for a in arms.iter() {
                                let target_idx = self.alloc_idx();
                                let typed_bindings = pattern_binding_types(
                                    &a.pattern,
                                    scrut_ty.as_ref(),
                                    self.enums,
                                );
                                let arm_block = match &a.body.kind {
                                    ExprKind::Block(b) => b.clone(),
                                    _ => Block {
                                        stmts: Vec::new(),
                                        tail: Some(Box::new(a.body.clone())),
                                    },
                                };
                                term_arms.push(MatchTArm {
                                    pattern: a.pattern.clone(),
                                    target_idx,
                                });
                                per_arm.push((target_idx, typed_bindings, arm_block));
                            }
                            self.push_seg(
                                idx,
                                cumulative_fields,
                                std::mem::take(&mut cur_stmts),
                                SegTerm::MatchT {
                                    scrutinee: (**scrutinee).clone(),
                                    arms: term_arms,
                                },
                            );
                            for (target_idx, typed_bindings, arm_block) in per_arm {
                                let mut arm_live = branch_live.clone();
                                for (b, t) in &typed_bindings {
                                    arm_live.push((*b, t.clone()));
                                }
                                self.build_block(
                                    &arm_block,
                                    target_idx,
                                    arm_live,
                                    FinalTerm::JumpBindTail { target_idx: join_idx, binding: *name },
                                );
                            }
                        }
                        _ => unreachable!("join_kind matched but RHS isn't If/Match"),
                    }
                    // Continue the outer block at join_idx with `X`
                    // added as a live local. Tail-recurse so the
                    // remainder of stmts + tail flow through.
                    let rest = Block {
                        stmts: stmts_slice[stmt_i..].to_vec(),
                        tail: block.tail.clone(),
                    };
                    let mut join_live = branch_live;
                    join_live.push((*name, r_ty));
                    self.build_block(&rest, join_idx, join_live, final_term);
                    return;
                }
            }
            // `let X = await E` — Suspend terminator boundary.
            if let StmtKind::Let { name, value, .. } = &s.kind {
                if let ExprKind::Await(p) = &value.kind {
                    let binding_ty =
                        self.let_ty.get(name).cloned().unwrap_or(Type::I64);
                    let next_idx = self.alloc_idx();
                    self.push_seg(
                        idx,
                        cumulative_fields.clone(),
                        std::mem::take(&mut cur_stmts),
                        SegTerm::Suspend {
                            promise: (**p).clone(),
                            binding: *name,
                            binding_ty: binding_ty.clone(),
                            next_idx,
                        },
                    );
                    cumulative_fields.append(&mut pending_lets);
                    cumulative_fields.push((*name, binding_ty));
                    idx = next_idx;
                    continue;
                }
            }
            // Stmt-position `if cond { ...await... } [else { ... }]`.
            // The if produces no value (stmt position); both arms
            // converge on a synthesized after segment that picks up
            // the remaining stmts/tail of the outer block.
            if let StmtKind::Expr(e) = &s.kind {
                if let ExprKind::If { cond, then_branch, else_branch } = &e.kind {
                    let any_await = block_has_await(then_branch)
                        || else_branch.as_deref().is_some_and(expr_has_await);
                    if any_await {
                        let then_idx = self.alloc_idx();
                        let else_idx = self.alloc_idx();
                        let after_idx = self.alloc_idx();
                        let mut branch_live = cumulative_fields.clone();
                        branch_live.append(&mut pending_lets);
                        self.push_seg(
                            idx,
                            cumulative_fields,
                            std::mem::take(&mut cur_stmts),
                            SegTerm::Branch {
                                cond: (**cond).clone(),
                                then_idx,
                                else_idx,
                            },
                        );
                        self.build_block(
                            then_branch,
                            then_idx,
                            branch_live.clone(),
                            FinalTerm::JumpTo(after_idx),
                        );
                        if let Some(eb) = else_branch.as_deref() {
                            let else_blk = coerce_to_block(eb);
                            self.build_block(
                                &else_blk,
                                else_idx,
                                branch_live.clone(),
                                FinalTerm::JumpTo(after_idx),
                            );
                        } else {
                            // Synth empty else segment: jump straight to after.
                            self.push_seg(
                                else_idx,
                                branch_live.clone(),
                                Vec::new(),
                                SegTerm::Jump { target_idx: after_idx },
                            );
                        }
                        // Continue outer block at after_idx with the
                        // same cumulative_fields (the if-stmt's arms
                        // don't introduce escaping bindings).
                        cumulative_fields = branch_live;
                        idx = after_idx;
                        continue;
                    }
                }
            }
            // While-with-await statement.
            if let StmtKind::Expr(e) = &s.kind {
                if let ExprKind::While { cond, body } = &e.kind {
                    if block_has_await(body) {
                        // Seal the pre-loop segment with a Jump to
                        // the header so resumption from a prior
                        // Suspend reliably hits it.
                        let header_idx = self.alloc_idx();
                        let body_idx = self.alloc_idx();
                        let after_idx = self.alloc_idx();
                        // Flush pending sync lets — they're live in
                        // the header / body / after.
                        let mut live = cumulative_fields.clone();
                        live.append(&mut pending_lets);
                        self.push_seg(
                            idx,
                            cumulative_fields.clone(),
                            std::mem::take(&mut cur_stmts),
                            SegTerm::Jump { target_idx: header_idx },
                        );
                        // Header segment: evaluate cond, branch.
                        self.push_seg(
                            header_idx,
                            live.clone(),
                            Vec::new(),
                            SegTerm::Branch {
                                cond: (**cond).clone(),
                                then_idx: body_idx,
                                else_idx: after_idx,
                            },
                        );
                        // Recurse into the body with the loop ctx
                        // set so segments built inside inherit
                        // loop_info, and with final_term = Jump back
                        // to the header.
                        let saved_loop = self.cur_loop;
                        self.cur_loop = Some((header_idx, after_idx));
                        self.build_block(
                            body,
                            body_idx,
                            live.clone(),
                            FinalTerm::JumpTo(header_idx),
                        );
                        self.cur_loop = saved_loop;
                        // Continue outer block at after_idx with the
                        // same cumulative_fields as at while-entry
                        // (loop-body lets don't escape).
                        cumulative_fields = live;
                        idx = after_idx;
                        continue;
                    }
                }
            }
            if let StmtKind::Let { name, ty, .. } = &s.kind {
                let resolved = ty
                    .clone()
                    .or_else(|| self.let_ty.get(name).cloned())
                    .unwrap_or(Type::I64);
                pending_lets.push((*name, resolved));
            }
            cur_stmts.push(s.clone());
        }

        // Tail handling: tail-Match with awaits → MatchT terminator.
        if let Some(t) = block.tail.as_deref() {
            if let ExprKind::Match { scrutinee, arms } = &t.kind {
                let has_arm_await = arms.iter().any(|a| expr_has_await(&a.body));
                if has_arm_await {
                    let mut branch_live = cumulative_fields.clone();
                    branch_live.append(&mut pending_lets);
                    let scrut_ty = resolve_var_ty(scrutinee, &branch_live);
                    let mut term_arms: Vec<MatchTArm> = Vec::new();
                    let mut per_arm: Vec<(u32, Vec<(Symbol, Type)>, Block)> = Vec::new();
                    for a in arms.iter() {
                        let target_idx = self.alloc_idx();
                        let typed_bindings = pattern_binding_types(
                            &a.pattern,
                            scrut_ty.as_ref(),
                            self.enums,
                        );
                        let arm_block = match &a.body.kind {
                            ExprKind::Block(b) => b.clone(),
                            _ => Block {
                                stmts: Vec::new(),
                                tail: Some(Box::new(a.body.clone())),
                            },
                        };
                        term_arms.push(MatchTArm {
                            pattern: a.pattern.clone(),
                            target_idx,
                        });
                        per_arm.push((target_idx, typed_bindings, arm_block));
                    }
                    self.push_seg(
                        idx,
                        cumulative_fields,
                        cur_stmts,
                        SegTerm::MatchT {
                            scrutinee: (**scrutinee).clone(),
                            arms: term_arms,
                        },
                    );
                    for (target_idx, typed_bindings, arm_block) in per_arm {
                        let mut arm_live = branch_live.clone();
                        for (b, t) in &typed_bindings {
                            arm_live.push((*b, t.clone()));
                        }
                        self.build_block(&arm_block, target_idx, arm_live, final_term);
                    }
                    return;
                }
            }
            if let ExprKind::If { cond, then_branch, else_branch } = &t.kind {
                let has_branch_await =
                    block_has_await(then_branch)
                        || else_branch
                            .as_deref()
                            .is_some_and(expr_has_await);
                if has_branch_await {
                    let mut branch_live = cumulative_fields.clone();
                    branch_live.append(&mut pending_lets);
                    let then_idx = self.alloc_idx();
                    let else_idx = self.alloc_idx();
                    self.push_seg(
                        idx,
                        cumulative_fields,
                        cur_stmts,
                        SegTerm::Branch {
                            cond: (**cond).clone(),
                            then_idx,
                            else_idx,
                        },
                    );
                    self.build_block(then_branch, then_idx, branch_live.clone(), final_term);
                    if let Some(eb) = else_branch.as_deref() {
                        let else_blk = coerce_to_block(eb);
                        self.build_block(&else_blk, else_idx, branch_live, final_term);
                    }
                    return;
                }
            }
        }

        // Default cap: depends on final_term.
        let mut final_stmts = cur_stmts;
        let term = match final_term {
            FinalTerm::SettleTail => {
                let tail_val = block
                    .tail
                    .as_deref()
                    .cloned()
                    .unwrap_or_else(|| mk_int(0, self.span));
                SegTerm::Settle { value: tail_val }
            }
            FinalTerm::JumpTo(target) => {
                // Loop body: tail value is discarded but the tail
                // expr may have side effects (`if cond { break }`).
                if let Some(t) = block.tail.as_deref() {
                    final_stmts.push(mk_expr_stmt(t.clone(), self.span));
                }
                SegTerm::Jump { target_idx: target }
            }
            FinalTerm::JumpBindTail { target_idx, binding } => {
                // Mid-body join arm: the block's tail is the value
                // bound into `binding` at the join variant.
                let value = block
                    .tail
                    .as_deref()
                    .cloned()
                    .unwrap_or_else(|| mk_int(0, self.span));
                SegTerm::JumpBind { target_idx, binding, value }
            }
        };
        self.push_seg(idx, cumulative_fields, final_stmts, term);
    }
}

/// Walk a (Phase-2a supported) body and produce one Segment per
/// state. `body_lets` is the (name, type) list previously computed
/// for the class-based lowering — we reuse it for liveness over-approx.
pub fn build_segments(
    body: &Block,
    params: &[Param],
    body_lets: &[(Symbol, Type)],
    inner_ret: &Type,
    enclosing_class: Option<Symbol>,
    span: Span,
    enums: &HashMap<Symbol, EnumDecl>,
) -> Vec<Segment> {
    let _ = inner_ret;
    // V_0's live-in: params (+ __this if class method).
    let mut initial_fields: Vec<(Symbol, Type)> = Vec::new();
    if let Some(class) = enclosing_class {
        initial_fields.push((Symbol::intern("__this"), Type::Object(class)));
    }
    for p in params {
        initial_fields.push((p.name, p.ty.clone()));
    }

    let let_ty: HashMap<Symbol, Type> = body_lets.iter().cloned().collect();
    let mut builder = SegBuilder {
        segments: Vec::new(),
        next_idx: 1,
        let_ty: &let_ty,
        span,
        cur_loop: None,
        enums,
    };
    builder.build_block(body, 0, initial_fields, FinalTerm::SettleTail);
    builder.segments
}

// --- AST generators -------------------------------------------------

/// Generate the state enum: one variant per segment, each carrying
/// the segment's field layout as a Struct payload.
pub fn gen_state_enum(
    enum_name: Symbol,
    segments: &[Segment],
    span: Span,
) -> EnumDecl {
    let mut variants: Vec<Variant> = Vec::new();
    for seg in segments {
        let fields: Vec<FieldDecl> = seg
            .fields
            .iter()
            .map(|(n, t)| FieldDecl {
                is_pub: true,
                name: *n,
                ty: t.clone(),
                span,
                bits: None,
            })
            .collect();
        variants.push(Variant {
            name: Symbol::intern(&format!("S{}", seg.idx)),
            payload: VariantPayload::Struct(fields.into_boxed_slice()),
            discriminant: None,
            span,
        });
    }
    EnumDecl {
        is_pub: false,
        name: enum_name,
        type_params: Box::new([]),
        repr_ty: None,
        flags: false,
        variants: variants.into_boxed_slice(),
        span,
    }
}

/// Generate the state-ref class. Two fields: `current: EnumT` and
/// `__async_promise: Promise<T>`. The init writes both verbatim.
pub fn gen_state_ref_class(
    class_name: Symbol,
    enum_name: Symbol,
    promise_ret: &Type,
    span: Span,
) -> ClassDecl {
    let enum_ty = Type::Object(enum_name);
    let fields = vec![
        FieldDecl {
            is_pub: true,
            name: Symbol::intern("current"),
            ty: enum_ty.clone(),
            span,
            bits: None,
        },
        FieldDecl {
            is_pub: true,
            name: Symbol::intern("__async_promise"),
            ty: promise_ret.clone(),
            span,
            bits: None,
        },
    ];
    let init_initial = Symbol::intern("__init_state");
    let init_prom = Symbol::intern("__init_prom");
    let init_params = vec![
        Param {
            name: init_initial,
            ty: enum_ty.clone(),
            span,
            default: None,
        },
        Param {
            name: init_prom,
            ty: promise_ret.clone(),
            span,
            default: None,
        },
    ];
    let this_e = || Expr::new(ExprKind::This, span);
    let init_stmts = vec![
        mk_expr_stmt(
            mk_assign_field(
                this_e(),
                Symbol::intern("current"),
                mk_var(init_initial, span),
                span,
            ),
            span,
        ),
        mk_expr_stmt(
            mk_assign_field(
                this_e(),
                Symbol::intern("__async_promise"),
                mk_var(init_prom, span),
                span,
            ),
            span,
        ),
    ];
    let init_method = FnDecl {
        attrs: Box::new([]),
        is_pub: true,
        name: Symbol::intern("init"),
        type_params: Box::new([]),
        params: init_params.into_boxed_slice(),
        ret: None,
        body: Block { stmts: init_stmts, tail: None },
        span,
        is_override: false,
        is_async: false,
    };
    ClassDecl {
        extern_lib: None,
        is_repr_c: false,
        is_packed: false,
        is_union: false,
        is_pub: false,
        name: class_name,
        parent: None,
        interfaces: Box::new([]),
        type_params: Box::new([]),
        fields: fields.into_boxed_slice(),
        methods: Box::new([init_method]),
        static_methods: Box::new([]),
        static_fields: Box::new([]),
        properties: Box::new([]),
        span,
    }
}

/// Generate the poll fn. Body: `loop { match state_ref.current { ... } }`
/// where each arm runs the segment's stmts, then either suspends
/// (`.then` registration + return) or settles + returns.
pub fn gen_poll_fn(
    poll_name: Symbol,
    state_ref_class: Symbol,
    state_enum: Symbol,
    segments: &[Segment],
    enclosing_class: Option<Symbol>,
    span: Span,
) -> FnDecl {
    let state_ref_param = Symbol::intern("__state_ref");
    let dummy_awaited_param = Symbol::intern("__awaited_value");

    // Build a `idx -> &Segment` lookup. Segments are appended to
    // the vec in DFS push order, not variant-index order (the
    // Branch terminator allocates `then_idx` / `else_idx` before
    // recursing into either branch), so direct `segments[idx]` is
    // wrong.
    let mut by_idx: Vec<Option<&Segment>> =
        std::iter::repeat_with(|| None).take(segments.len()).collect();
    for s in segments {
        let i = s.idx as usize;
        if i >= by_idx.len() {
            // Defensive: grow if needed (shouldn't happen given
            // segments.len() == # variants).
            by_idx.resize_with(i + 1, || None);
        }
        by_idx[i] = Some(s);
    }
    let by_idx: Vec<&Segment> = by_idx.into_iter().flatten().collect();

    let mut match_arms: Vec<MatchArm> = Vec::new();
    for seg in segments {
        let arm_body = emit_segment_arm(
            seg,
            &by_idx,
            state_ref_param,
            poll_name,
            state_enum,
            enclosing_class,
            span,
        );
        let pattern = Pattern {
            kind: PatternKind::Variant {
                enum_name: Some(state_enum),
                variant: Symbol::intern(&format!("S{}", seg.idx)),
                bindings: PatternBindings::Struct(
                    seg.fields
                        .iter()
                        .map(|(n, _)| (*n, *n))
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                ),
            },
            span,
        };
        match_arms.push(MatchArm {
            pattern,
            body: Expr::new(ExprKind::Block(arm_body), span),
            span,
        });
    }

    let match_expr = Expr::new(
        ExprKind::Match {
            scrutinee: Box::new(mk_field(
                mk_var(state_ref_param, span),
                Symbol::intern("current"),
                span,
            )),
            arms: match_arms.into_boxed_slice(),
        },
        span,
    );
    let loop_body = Block {
        stmts: vec![mk_expr_stmt(match_expr, span)],
        tail: None,
    };
    let body = Block {
        stmts: vec![mk_expr_stmt(
            Expr::new(ExprKind::Loop { body: loop_body }, span),
            span,
        )],
        tail: None,
    };
    FnDecl {
        attrs: Box::new([]),
        is_pub: false,
        name: poll_name,
        type_params: Box::new([]),
        params: Box::new([
            Param {
                name: state_ref_param,
                ty: Type::Object(state_ref_class),
                span,
                default: None,
            },
            Param {
                name: dummy_awaited_param,
                ty: Type::I64,
                span,
                default: None,
            },
        ]),
        ret: None,
        body,
        span,
        is_override: false,
        is_async: false,
    }
}

/// Build one segment's arm body. Runs the segment's sync stmts,
/// then handles the terminator (Suspend or Settle), then `return`s
/// to exit the outer poll fn (preventing the `loop { ... }` from
/// iterating again).
/// Build a "transition to `target_idx` and re-enter __poll" Block:
/// `{ state_ref.current = S{target}{...locals...}; __poll(state_ref, 0); return; }`.
/// The ctor args are pulled from the destination variant's field list
/// — each one defaults to `Var(field_name)` (the local must be in
/// scope), but `overrides` may supply an arbitrary expression for
/// specific fields (used by `JumpBind` to thread the arm's tail
/// value into a join variant's binding field).
fn mk_transition_block_override(
    target_idx: u32,
    all_segments: &[&Segment],
    state_ref_param: Symbol,
    poll_name: Symbol,
    state_enum: Symbol,
    span: Span,
    overrides: &[(Symbol, Expr)],
) -> Block {
    let target_seg = &all_segments[target_idx as usize];
    let ctor_args: Vec<(Symbol, Expr)> = target_seg
        .fields
        .iter()
        .map(|(n, _)| {
            if let Some((_, v)) = overrides.iter().find(|(name, _)| name == n) {
                let mut e = v.clone();
                rewrite_this_in_expr(&mut e);
                (*n, e)
            } else {
                let mut e = mk_var(*n, span);
                rewrite_this_in_expr(&mut e);
                (*n, e)
            }
        })
        .collect();
    let new_variant = mk_enum_ctor_struct(
        state_enum,
        Symbol::intern(&format!("S{}", target_idx)),
        ctor_args,
        span,
    );
    Block {
        stmts: vec![
            mk_expr_stmt(
                mk_assign_field(
                    mk_var(state_ref_param, span),
                    Symbol::intern("current"),
                    new_variant,
                    span,
                ),
                span,
            ),
            mk_expr_stmt(
                mk_call(
                    poll_name,
                    vec![mk_var(state_ref_param, span), mk_int(0, span)],
                    span,
                ),
                span,
            ),
            mk_expr_stmt(
                Expr::new(ExprKind::Return(None), span),
                span,
            ),
        ],
        tail: None,
    }
}

/// Shorthand for `mk_transition_block_override` with no overrides.
fn mk_transition_block(
    target_idx: u32,
    all_segments: &[&Segment],
    state_ref_param: Symbol,
    poll_name: Symbol,
    state_enum: Symbol,
    span: Span,
) -> Block {
    mk_transition_block_override(
        target_idx, all_segments, state_ref_param, poll_name, state_enum, span, &[],
    )
}

/// Walk `b` and replace every `break` / `continue` (at any nesting
/// depth, but NOT crossing a nested loop) with a transition Block to
/// `after_idx` / `header_idx` respectively. Phase 2b doesn't allow
/// nested loops inside an async while body, so we don't currently
/// need to skip nested while bodies — but the walker is defensive
/// and stops at them.
fn rewrite_loop_jumps_block(
    b: &mut Block,
    header_idx: u32,
    after_idx: u32,
    all_segments: &[&Segment],
    state_ref_param: Symbol,
    poll_name: Symbol,
    state_enum: Symbol,
    span: Span,
) {
    for s in b.stmts.iter_mut() {
        rewrite_loop_jumps_stmt(
            s, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum, span,
        );
    }
    if let Some(t) = b.tail.as_mut() {
        rewrite_loop_jumps_expr(
            t, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum, span,
        );
    }
}

fn rewrite_loop_jumps_stmt(
    s: &mut Stmt,
    header_idx: u32,
    after_idx: u32,
    all_segments: &[&Segment],
    state_ref_param: Symbol,
    poll_name: Symbol,
    state_enum: Symbol,
    span: Span,
) {
    match &mut s.kind {
        StmtKind::Let { value, .. }
        | StmtKind::LetTuple { value, .. }
        | StmtKind::LetStruct { value, .. } => rewrite_loop_jumps_expr(
            value, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum,
            span,
        ),
        StmtKind::Expr(e) => rewrite_loop_jumps_expr(
            e, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum, span,
        ),
    }
}

fn rewrite_loop_jumps_expr(
    e: &mut Expr,
    header_idx: u32,
    after_idx: u32,
    all_segments: &[&Segment],
    state_ref_param: Symbol,
    poll_name: Symbol,
    state_enum: Symbol,
    span: Span,
) {
    match &mut e.kind {
        ExprKind::Break(_) => {
            let blk = mk_transition_block(
                after_idx, all_segments, state_ref_param, poll_name, state_enum, span,
            );
            e.kind = ExprKind::Block(blk);
        }
        ExprKind::Continue => {
            let blk = mk_transition_block(
                header_idx, all_segments, state_ref_param, poll_name, state_enum, span,
            );
            e.kind = ExprKind::Block(blk);
        }
        // Nested loops would shadow break/continue — stop recursing.
        ExprKind::While { .. } | ExprKind::Loop { .. } | ExprKind::ForIn { .. } => {}
        ExprKind::Block(b) => rewrite_loop_jumps_block(
            b, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum, span,
        ),
        ExprKind::If { cond, then_branch, else_branch } => {
            rewrite_loop_jumps_expr(
                cond, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum,
                span,
            );
            rewrite_loop_jumps_block(
                then_branch, header_idx, after_idx, all_segments, state_ref_param, poll_name,
                state_enum, span,
            );
            if let Some(eb) = else_branch {
                rewrite_loop_jumps_expr(
                    eb, header_idx, after_idx, all_segments, state_ref_param, poll_name,
                    state_enum, span,
                );
            }
        }
        ExprKind::IfLet { expr, then_branch, else_branch, .. } => {
            rewrite_loop_jumps_expr(
                expr, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum,
                span,
            );
            rewrite_loop_jumps_block(
                then_branch, header_idx, after_idx, all_segments, state_ref_param, poll_name,
                state_enum, span,
            );
            if let Some(eb) = else_branch {
                rewrite_loop_jumps_expr(
                    eb, header_idx, after_idx, all_segments, state_ref_param, poll_name,
                    state_enum, span,
                );
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            rewrite_loop_jumps_expr(
                scrutinee, header_idx, after_idx, all_segments, state_ref_param, poll_name,
                state_enum, span,
            );
            for a in arms.iter_mut() {
                rewrite_loop_jumps_expr(
                    &mut a.body, header_idx, after_idx, all_segments, state_ref_param, poll_name,
                    state_enum, span,
                );
            }
        }
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
            rewrite_loop_jumps_expr(
                lhs, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum,
                span,
            );
            rewrite_loop_jumps_expr(
                rhs, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum,
                span,
            );
        }
        ExprKind::Unary { expr, .. }
        | ExprKind::Cast { expr, .. }
        | ExprKind::TypeTest { expr, .. }
        | ExprKind::TypeDowncast { expr, .. } => rewrite_loop_jumps_expr(
            expr, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum,
            span,
        ),
        ExprKind::Some(inner) | ExprKind::Await(inner) => rewrite_loop_jumps_expr(
            inner, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum,
            span,
        ),
        ExprKind::Return(opt) => {
            if let Some(inner) = opt {
                rewrite_loop_jumps_expr(
                    inner, header_idx, after_idx, all_segments, state_ref_param, poll_name,
                    state_enum, span,
                );
            }
        }
        ExprKind::Assign { value, .. } => rewrite_loop_jumps_expr(
            value, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum,
            span,
        ),
        ExprKind::AssignField { obj, value, .. } => {
            rewrite_loop_jumps_expr(
                obj, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum,
                span,
            );
            rewrite_loop_jumps_expr(
                value, header_idx, after_idx, all_segments, state_ref_param, poll_name,
                state_enum, span,
            );
        }
        ExprKind::AssignIndex { obj, index, value } => {
            rewrite_loop_jumps_expr(
                obj, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum,
                span,
            );
            rewrite_loop_jumps_expr(
                index, header_idx, after_idx, all_segments, state_ref_param, poll_name,
                state_enum, span,
            );
            rewrite_loop_jumps_expr(
                value, header_idx, after_idx, all_segments, state_ref_param, poll_name,
                state_enum, span,
            );
        }
        ExprKind::Call { args, .. }
        | ExprKind::SuperCall { args, .. }
        | ExprKind::New { args, .. } => {
            for a in args.iter_mut() {
                rewrite_loop_jumps_expr(
                    a, header_idx, after_idx, all_segments, state_ref_param, poll_name,
                    state_enum, span,
                );
            }
        }
        ExprKind::MethodCall { obj, args, .. } => {
            rewrite_loop_jumps_expr(
                obj, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum,
                span,
            );
            for a in args.iter_mut() {
                rewrite_loop_jumps_expr(
                    a, header_idx, after_idx, all_segments, state_ref_param, poll_name,
                    state_enum, span,
                );
            }
        }
        ExprKind::Field { obj, .. } => rewrite_loop_jumps_expr(
            obj, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum,
            span,
        ),
        ExprKind::Index { obj, index } => {
            rewrite_loop_jumps_expr(
                obj, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum,
                span,
            );
            rewrite_loop_jumps_expr(
                index, header_idx, after_idx, all_segments, state_ref_param, poll_name,
                state_enum, span,
            );
        }
        ExprKind::Tuple(es) | ExprKind::Array(es) => {
            for e in es.iter_mut() {
                rewrite_loop_jumps_expr(
                    e, header_idx, after_idx, all_segments, state_ref_param, poll_name,
                    state_enum, span,
                );
            }
        }
        ExprKind::FnExpr { body, .. } => rewrite_loop_jumps_block(
            body, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum,
            span,
        ),
        _ => {}
    }
}

fn emit_segment_arm(
    seg: &Segment,
    all_segments: &[&Segment],
    state_ref_param: Symbol,
    poll_name: Symbol,
    state_enum: Symbol,
    _enclosing_class: Option<Symbol>,
    span: Span,
) -> Block {
    let mut stmts: Vec<Stmt> = Vec::new();
    for s in &seg.stmts {
        let mut s2 = s.clone();
        rewrite_this_in_stmt(&mut s2);
        // If this segment lives inside a loop, rewrite break/continue
        // inside its stmts to transition Blocks.
        if let Some((header_idx, after_idx)) = seg.loop_info {
            rewrite_loop_jumps_stmt(
                &mut s2,
                header_idx,
                after_idx,
                all_segments,
                state_ref_param,
                poll_name,
                state_enum,
                span,
            );
        }
        stmts.push(s2);
    }
    match &seg.terminator {
        SegTerm::Suspend {
            promise,
            binding,
            binding_ty,
            next_idx,
        } => {
            // The continuation closure builds the next variant
            // with: (a) the binding name = the resolved value,
            // (b) every other field of the next variant carried
            // over from the current arm body's locals.
            //
            // The destination variant's field set is computed at
            // build time but we don't have it here — the segments
            // vec encodes it on segment[next_idx]. We approximate
            // by including every current-arm local that the
            // surrounding fn has visibility of.
            // (See enum_state_machine module's call site for the
            // resolved next_fields list.)
            //
            // For now we pass the resolved next_fields via the
            // SegTerm; emit it as the EnumCtor args.
            // Build ctor args for V_{next}: every field of the next
            // variant. For the await binding itself, use the closure
            // parameter; for all others, use the local of the same
            // name (either destructured from V_K or bound by S_K's
            // sync stmts above).
            let next_seg = &all_segments[*next_idx as usize];
            let mut ctor_args: Vec<(Symbol, Expr)> = next_seg
                .fields
                .iter()
                .map(|(n, _)| {
                    if n == binding {
                        (*n, mk_var(*n, span))
                    } else {
                        let mut expr = mk_var(*n, span);
                        rewrite_this_in_expr(&mut expr);
                        (*n, expr)
                    }
                })
                .collect();
            // If next_seg didn't include `binding` (shouldn't happen),
            // append defensively.
            if !next_seg.fields.iter().any(|(n, _)| n == binding) {
                ctor_args.push((*binding, mk_var(*binding, span)));
            }

            let mut prom_expr = promise.clone();
            rewrite_this_in_expr(&mut prom_expr);

            let v_name = *binding;
            let new_variant = mk_enum_ctor_struct(
                state_enum,
                Symbol::intern(&format!("S{}", next_idx)),
                ctor_args,
                span,
            );
            let closure_body = Block {
                stmts: vec![
                    mk_expr_stmt(
                        mk_assign_field(
                            mk_var(state_ref_param, span),
                            Symbol::intern("current"),
                            new_variant,
                            span,
                        ),
                        span,
                    ),
                    mk_expr_stmt(
                        mk_call(
                            poll_name,
                            vec![mk_var(state_ref_param, span), mk_int(0, span)],
                            span,
                        ),
                        span,
                    ),
                ],
                tail: Some(Box::new(mk_var(v_name, span))),
            };
            let closure = Expr::new(
                ExprKind::FnExpr {
                    params: Box::new([Param {
                        name: v_name,
                        ty: binding_ty.clone(),
                        span,
                        default: None,
                    }]),
                    ret: Some(binding_ty.clone()),
                    body: closure_body,
                },
                span,
            );
            let then_call = mk_method_call(
                prom_expr,
                Symbol::intern("then"),
                vec![closure],
                span,
            );
            stmts.push(mk_let(Symbol::intern("_"), None, then_call, span));
            stmts.push(mk_expr_stmt(
                Expr::new(ExprKind::Return(None), span),
                span,
            ));
        }
        SegTerm::Branch { cond, then_idx, else_idx } => {
            let mut cond_e = cond.clone();
            rewrite_this_in_expr(&mut cond_e);
            // If this Branch lives inside a loop body (only happens
            // when the Branch is itself the tail of the loop body —
            // not currently emitted but supported defensively),
            // rewrite break/continue in cond too.
            if let Some((header_idx, after_idx)) = seg.loop_info {
                rewrite_loop_jumps_expr(
                    &mut cond_e,
                    header_idx,
                    after_idx,
                    all_segments,
                    state_ref_param,
                    poll_name,
                    state_enum,
                    span,
                );
            }
            let then_blk = mk_transition_block(
                *then_idx, all_segments, state_ref_param, poll_name, state_enum, span,
            );
            let else_blk = mk_transition_block(
                *else_idx, all_segments, state_ref_param, poll_name, state_enum, span,
            );
            let if_expr = Expr::new(
                ExprKind::If {
                    cond: Box::new(cond_e),
                    then_branch: then_blk,
                    else_branch: Some(Box::new(Expr::new(
                        ExprKind::Block(else_blk),
                        span,
                    ))),
                },
                span,
            );
            stmts.push(mk_expr_stmt(if_expr, span));
            stmts.push(mk_expr_stmt(
                Expr::new(ExprKind::Return(None), span),
                span,
            ));
        }
        SegTerm::Jump { target_idx } => {
            let blk = mk_transition_block(
                *target_idx, all_segments, state_ref_param, poll_name, state_enum, span,
            );
            for s in blk.stmts {
                stmts.push(s);
            }
        }
        SegTerm::JumpBind { target_idx, binding, value } => {
            let mut v = value.clone();
            rewrite_this_in_expr(&mut v);
            let blk = mk_transition_block_override(
                *target_idx,
                all_segments,
                state_ref_param,
                poll_name,
                state_enum,
                span,
                &[(*binding, v)],
            );
            for s in blk.stmts {
                stmts.push(s);
            }
        }
        SegTerm::MatchT { scrutinee, arms } => {
            let mut scrut_e = scrutinee.clone();
            rewrite_this_in_expr(&mut scrut_e);
            let match_arms: Vec<MatchArm> = arms
                .iter()
                .map(|a| {
                    let transition = mk_transition_block(
                        a.target_idx,
                        all_segments,
                        state_ref_param,
                        poll_name,
                        state_enum,
                        span,
                    );
                    MatchArm {
                        pattern: a.pattern.clone(),
                        body: Expr::new(ExprKind::Block(transition), span),
                        span,
                    }
                })
                .collect();
            let match_expr = Expr::new(
                ExprKind::Match {
                    scrutinee: Box::new(scrut_e),
                    arms: match_arms.into_boxed_slice(),
                },
                span,
            );
            stmts.push(mk_expr_stmt(match_expr, span));
            stmts.push(mk_expr_stmt(
                Expr::new(ExprKind::Return(None), span),
                span,
            ));
        }
        SegTerm::Settle { value } => {
            let mut v = value.clone();
            rewrite_this_in_expr(&mut v);
            stmts.push(mk_expr_stmt(
                mk_method_call(
                    mk_var(Symbol::intern("Promise"), span),
                    Symbol::intern("__settleResolve"),
                    vec![
                        mk_field(
                            mk_var(state_ref_param, span),
                            Symbol::intern("__async_promise"),
                            span,
                        ),
                        v,
                    ],
                    span,
                ),
                span,
            ));
            stmts.push(mk_expr_stmt(
                Expr::new(ExprKind::Return(None), span),
                span,
            ));
        }
    }
    Block { stmts, tail: None }
}

/// Rewrite every `this` to `Var(__this)` so the variant-destructured
/// local picks it up. Used in class-method lowering only; safe to
/// run for free async fns (their bodies don't contain `this`).
fn rewrite_this_in_stmt(s: &mut Stmt) {
    match &mut s.kind {
        StmtKind::Let { value, .. } => rewrite_this_in_expr(value),
        StmtKind::LetTuple { value, .. } | StmtKind::LetStruct { value, .. } => {
            rewrite_this_in_expr(value)
        }
        StmtKind::Expr(e) => rewrite_this_in_expr(e),
    }
}

fn rewrite_this_in_expr(e: &mut Expr) {
    match &mut e.kind {
        ExprKind::This => {
            e.kind = ExprKind::Var(Symbol::intern("__this"));
        }
        ExprKind::Block(b) => rewrite_this_in_block(b),
        ExprKind::If { cond, then_branch, else_branch } => {
            rewrite_this_in_expr(cond);
            rewrite_this_in_block(then_branch);
            if let Some(eb) = else_branch {
                rewrite_this_in_expr(eb);
            }
        }
        ExprKind::IfLet { expr, then_branch, else_branch, .. } => {
            rewrite_this_in_expr(expr);
            rewrite_this_in_block(then_branch);
            if let Some(eb) = else_branch {
                rewrite_this_in_expr(eb);
            }
        }
        ExprKind::While { cond, body } => {
            rewrite_this_in_expr(cond);
            rewrite_this_in_block(body);
        }
        ExprKind::Loop { body } => rewrite_this_in_block(body),
        ExprKind::ForIn { iter, body, .. } => {
            rewrite_this_in_expr(iter);
            rewrite_this_in_block(body);
        }
        ExprKind::Match { scrutinee, arms } => {
            rewrite_this_in_expr(scrutinee);
            for a in arms.iter_mut() {
                rewrite_this_in_expr(&mut a.body);
            }
        }
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
            rewrite_this_in_expr(lhs);
            rewrite_this_in_expr(rhs);
        }
        ExprKind::Unary { expr, .. }
        | ExprKind::Cast { expr, .. }
        | ExprKind::TypeTest { expr, .. }
        | ExprKind::TypeDowncast { expr, .. } => rewrite_this_in_expr(expr),
        ExprKind::Some(e) | ExprKind::Await(e) => rewrite_this_in_expr(e),
        ExprKind::Return(opt) | ExprKind::Break(opt) => {
            if let Some(e) = opt {
                rewrite_this_in_expr(e);
            }
        }
        ExprKind::Assign { value, .. } => rewrite_this_in_expr(value),
        ExprKind::AssignField { obj, value, .. } => {
            rewrite_this_in_expr(obj);
            rewrite_this_in_expr(value);
        }
        ExprKind::AssignIndex { obj, index, value } => {
            rewrite_this_in_expr(obj);
            rewrite_this_in_expr(index);
            rewrite_this_in_expr(value);
        }
        ExprKind::Call { args, .. }
        | ExprKind::SuperCall { args, .. }
        | ExprKind::New { args, .. } => {
            for a in args.iter_mut() {
                rewrite_this_in_expr(a);
            }
        }
        ExprKind::MethodCall { obj, args, .. } => {
            rewrite_this_in_expr(obj);
            for a in args.iter_mut() {
                rewrite_this_in_expr(a);
            }
        }
        ExprKind::Field { obj, .. } => rewrite_this_in_expr(obj),
        ExprKind::Index { obj, index } => {
            rewrite_this_in_expr(obj);
            rewrite_this_in_expr(index);
        }
        ExprKind::Tuple(es) | ExprKind::Array(es) => {
            for e in es.iter_mut() {
                rewrite_this_in_expr(e);
            }
        }
        ExprKind::FnExpr { body, .. } => rewrite_this_in_block(body),
        _ => {}
    }
}

fn rewrite_this_in_block(b: &mut Block) {
    for s in b.stmts.iter_mut() {
        rewrite_this_in_stmt(s);
    }
    if let Some(t) = b.tail.as_mut() {
        rewrite_this_in_expr(t);
    }
}

/// Generate the wrapper fn that allocates the StateRef + initial
/// variant + result promise, kicks `__<name>_poll(state_ref, 0)`,
/// and returns the result promise.
pub fn gen_wrapper_fn(
    orig: &FnDecl,
    state_ref_class: Symbol,
    state_enum: Symbol,
    poll_fn_name: Symbol,
    initial_fields: &[(Symbol, Type)],
    promise_ret: &Type,
    enclosing_class: Option<Symbol>,
    span: Span,
) -> FnDecl {
    let prom_local = Symbol::intern("__async_prom");
    let initial_local = Symbol::intern("__async_initial");
    let state_local = Symbol::intern("__async_state");

    let mut wrapper_stmts: Vec<Stmt> = Vec::new();
    wrapper_stmts.push(mk_let(
        prom_local,
        Some(promise_ret.clone()),
        mk_method_call(
            mk_var(Symbol::intern("Promise"), span),
            Symbol::intern("__pending"),
            vec![],
            span,
        ),
        span,
    ));
    // Initial variant ctor args: every field from V_0's field list.
    // For class methods, V_0 includes __this — pass `this` literal.
    let ctor_args: Vec<(Symbol, Expr)> = initial_fields
        .iter()
        .map(|(n, _)| {
            if enclosing_class.is_some() && n.as_str() == "__this" {
                (*n, Expr::new(ExprKind::This, span))
            } else {
                (*n, mk_var(*n, span))
            }
        })
        .collect();
    wrapper_stmts.push(mk_let(
        initial_local,
        None,
        mk_enum_ctor_struct(state_enum, Symbol::intern("S0"), ctor_args, span),
        span,
    ));
    wrapper_stmts.push(mk_let(
        state_local,
        None,
        Expr::new(
            ExprKind::New {
                class: state_ref_class,
                type_args: Box::new([]),
                args: Box::new([
                    mk_var(initial_local, span),
                    mk_var(prom_local, span),
                ]),
                init_method: None,
            },
            span,
        ),
        span,
    ));
    wrapper_stmts.push(mk_expr_stmt(
        mk_call(
            poll_fn_name,
            vec![mk_var(state_local, span), mk_int(0, span)],
            span,
        ),
        span,
    ));
    let wrapper_body = Block {
        stmts: wrapper_stmts,
        tail: Some(Box::new(mk_var(prom_local, span))),
    };
    FnDecl {
        attrs: orig.attrs.clone(),
        is_pub: orig.is_pub,
        name: orig.name,
        type_params: orig.type_params.clone(),
        params: orig.params.clone(),
        ret: Some(promise_ret.clone()),
        body: wrapper_body,
        span: orig.span,
        is_override: orig.is_override,
        is_async: false,
    }
}

/// Top-level orchestrator. Returns the four generated items if the
/// body is a Phase-1 supported shape; otherwise returns
/// `NeedsFallback` and the caller routes to the legacy class-based
/// path.
pub fn lower(
    f: &FnDecl,
    body_lets: &[(Symbol, Type)],
    enclosing_class: Option<Symbol>,
    enums: &HashMap<Symbol, EnumDecl>,
) -> LowerOutput {
    if !body_is_supported(&f.body) {
        return LowerOutput::NeedsFallback;
    }
    let _ = body_is_straight_line; // kept for reference / future use
    let span = f.span;
    let inner_ret = f.ret.clone().unwrap_or(Type::Unit);
    let promise_ret = Type::generic("Promise", vec![inner_ret.clone()]);

    let segments =
        build_segments(&f.body, &f.params, body_lets, &inner_ret, enclosing_class, span, enums);
    if segments.len() <= 1 {
        return LowerOutput::NoAwait;
    }

    // Naming. For class methods, prefix with the class name to
    // avoid collision with same-named methods on other classes.
    let (enum_name, ref_name, poll_name) = match enclosing_class {
        Some(class) => (
            Symbol::intern(&format!("__{}_{}_State", class.as_str(), f.name.as_str())),
            Symbol::intern(&format!("__{}_{}_StateRef", class.as_str(), f.name.as_str())),
            Symbol::intern(&format!("__{}_{}_poll", class.as_str(), f.name.as_str())),
        ),
        None => (
            Symbol::intern(&format!("__{}_State", f.name.as_str())),
            Symbol::intern(&format!("__{}_StateRef", f.name.as_str())),
            Symbol::intern(&format!("__{}_poll", f.name.as_str())),
        ),
    };

    let state_enum = gen_state_enum(enum_name, &segments, span);
    let state_ref_class = gen_state_ref_class(ref_name, enum_name, &promise_ret, span);
    let poll_fn = gen_poll_fn(poll_name, ref_name, enum_name, &segments, enclosing_class, span);
    let wrapper = gen_wrapper_fn(
        f,
        ref_name,
        enum_name,
        poll_name,
        &segments[0].fields,
        &promise_ret,
        enclosing_class,
        span,
    );

    LowerOutput::Built(StateMachineOutput {
        wrapper,
        state_enum,
        state_ref_class,
        poll_fn,
    })
}
