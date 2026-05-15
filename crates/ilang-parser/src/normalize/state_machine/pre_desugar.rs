//! AST pre-passes that normalize various loop forms into a `while`
//! shape the segment builder understands.
//!
//! Three independent rewrites, applied in order from
//! `async_desugar::lower_async_fn`:
//!
//! 1. `loop { ...await... }` → `while true { ... }`.
//! 2. `while await cond { body }` → `while true { let cv = await cond;
//!    if !cv { break }; body }` (so cond re-evaluates per iter).
//! 3. `for v in s..e { body }` / `for v in s.. { body }` /
//!    `for v in arr { body }` → an index-based `while` over the
//!    appropriate bound.

use ilang_ast::{Block, Expr, ExprKind, Stmt, StmtKind, Symbol};

use super::{block_has_await, expr_has_await};

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
// rewritten to an index-based while loop.

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
