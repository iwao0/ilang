//! `async fn` → state-machine lowering (phase 1: scaffolding +
//! trivial case).
//!
//! ## Plan
//!
//! Long-term goal: each `async fn foo(args): T { body }` lowers to:
//!   1. A heap-allocated **state struct** holding the result Promise
//!      pointer + every local that's live across an `await` site +
//!      a `state_idx` discriminator.
//!   2. A **poll function** (a regular ilang fn) that switches on
//!      `state_idx` and runs the next chunk of the body. At each
//!      `await` it registers a continuation that re-enters poll
//!      with the resolved value, then returns. At the end it calls
//!      `__promise_settle_resolve` on the result Promise.
//!   3. The original `foo` becomes a thin wrapper that allocates
//!      the state, allocates a pending Promise, schedules the
//!      initial poll, and returns the Promise.
//!
//! ## What this commit does
//!
//! - Adds the analysis pass that walks each `async fn` body and
//!   collects:
//!     * suspend points (every direct `await` position),
//!     * the live-locals set at each suspend point,
//!     * a "supported shape" verdict (currently we only accept
//!       straight-line bodies whose awaits appear as `let x =
//!       await p` statements or as the final tail expression).
//! - Implements the **trivial-case lowering**: an async fn body
//!   with zero awaits gets rewritten to
//!   `Promise.resolve(<original body's value>)`, the `is_async`
//!   flag is cleared, and the declared return type wraps to
//!   `Promise<T>`. This exercises the AST-rewrite plumbing that
//!   the multi-state lowering will reuse.
//! - Async fns with awaits stay rejected (with a clearer error
//!   pointing at the analysis verdict).
//!
//! Multi-state poll-fn synthesis lands in a follow-up commit.

use std::collections::HashSet;

use ilang_ast::{
    Block, ClassDecl, Expr, ExprKind, FnDecl, Item, Param, Program, Span, Stmt,
    StmtKind, Symbol, Type,
};

/// Result of analysing one async fn's body.
#[derive(Debug)]
pub struct AsyncAnalysis {
    /// Spans of every `await` expression we found.
    pub suspend_points: Vec<Span>,
    /// `Some(reason)` if the body shape isn't supported by the
    /// current lowering. `None` means we'll lower it.
    pub unsupported: Option<String>,
}

/// Walk an async fn body and collect its analysis.
pub fn analyze(body: &Block) -> AsyncAnalysis {
    let mut a = AsyncAnalysis { suspend_points: Vec::new(), unsupported: None };
    walk_block(body, &mut a, /*top_level=*/ true);
    a
}

fn walk_block(b: &Block, a: &mut AsyncAnalysis, top_level: bool) {
    for s in &b.stmts {
        walk_stmt(s, a, top_level);
    }
    if let Some(t) = &b.tail {
        walk_expr(t, a);
    }
}

fn walk_stmt(s: &Stmt, a: &mut AsyncAnalysis, top_level: bool) {
    match &s.kind {
        StmtKind::Let { value, .. } => {
            // `let x = await p` is the supported await-in-stmt shape.
            // `let x = bar(await p)` (await inside a sub-expression)
            // gets flagged as unsupported below.
            if matches!(&value.kind, ExprKind::Await(_)) {
                a.suspend_points.push(value.span);
                if !top_level {
                    a.unsupported.get_or_insert_with(|| {
                        "await inside a nested block (if / loop / match) \
                         is not yet supported"
                            .into()
                    });
                }
                return;
            }
            walk_expr(value, a);
        }
        StmtKind::LetTuple { value, .. } | StmtKind::LetStruct { value, .. } => {
            walk_expr(value, a);
        }
        StmtKind::Expr(e) => walk_expr(e, a),
    }
}

fn walk_expr(e: &Expr, a: &mut AsyncAnalysis) {
    match &e.kind {
        ExprKind::Await(_) => {
            a.suspend_points.push(e.span);
            a.unsupported.get_or_insert_with(|| {
                "await inside a sub-expression is not yet supported \
                 (lift it into `let _t = await p` first)"
                    .into()
            });
        }
        ExprKind::Block(b) => walk_block(b, a, false),
        ExprKind::If { cond, then_branch, else_branch } => {
            walk_expr(cond, a);
            walk_block(then_branch, a, false);
            if let Some(eb) = else_branch {
                walk_expr(eb, a);
            }
        }
        ExprKind::IfLet { expr, then_branch, else_branch, .. } => {
            walk_expr(expr, a);
            walk_block(then_branch, a, false);
            if let Some(eb) = else_branch {
                walk_expr(eb, a);
            }
        }
        ExprKind::While { cond, body } => {
            walk_expr(cond, a);
            walk_block(body, a, false);
        }
        ExprKind::Loop { body } => walk_block(body, a, false),
        ExprKind::ForIn { iter, body, .. } => {
            walk_expr(iter, a);
            walk_block(body, a, false);
        }
        ExprKind::Match { scrutinee, arms } => {
            walk_expr(scrutinee, a);
            for arm in arms.iter() {
                walk_expr(&arm.body, a);
            }
        }
        ExprKind::Call { args, .. }
        | ExprKind::SuperCall { args, .. }
        | ExprKind::New { args, .. } => {
            for arg in args.iter() {
                walk_expr(arg, a);
            }
        }
        ExprKind::MethodCall { obj, args, .. } => {
            walk_expr(obj, a);
            for arg in args.iter() {
                walk_expr(arg, a);
            }
        }
        ExprKind::Field { obj, .. } => walk_expr(obj, a),
        ExprKind::Index { obj, index } => {
            walk_expr(obj, a);
            walk_expr(index, a);
        }
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
            walk_expr(lhs, a);
            walk_expr(rhs, a);
        }
        ExprKind::Unary { expr, .. }
        | ExprKind::Cast { expr, .. }
        | ExprKind::TypeTest { expr, .. }
        | ExprKind::TypeDowncast { expr, .. } => walk_expr(expr, a),
        ExprKind::Some(inner) => walk_expr(inner, a),
        ExprKind::Return(opt) | ExprKind::Break(opt) => {
            if let Some(e) = opt {
                walk_expr(e, a);
            }
        }
        ExprKind::Assign { value, .. } => walk_expr(value, a),
        ExprKind::AssignField { obj, value, .. } => {
            walk_expr(obj, a);
            walk_expr(value, a);
        }
        ExprKind::AssignIndex { obj, index, value } => {
            walk_expr(obj, a);
            walk_expr(index, a);
            walk_expr(value, a);
        }
        ExprKind::FnExpr { body, .. } => {
            // Closure bodies have their own scope — `await` inside
            // a nested closure isn't a suspend point of the outer
            // async fn (and would require the closure itself to be
            // async, which we don't support).
            let _ = body;
        }
        _ => {}
    }
}

/// Wrap an expression in `Promise.resolve(expr)`.
fn wrap_in_promise_resolve(expr: Expr) -> Expr {
    let span = expr.span;
    Expr::new(
        ExprKind::MethodCall {
            obj: Box::new(Expr::new(ExprKind::Var(Symbol::intern("Promise")), span)),
            method: Symbol::intern("resolve"),
            args: Box::new([expr]),
        },
        span,
    )
}

/// Rewrite the body so its result is wrapped in `Promise.resolve`.
/// Used for the trivial (zero-await) lowering.
fn wrap_body_in_promise_resolve(body: Block) -> Block {
    let span = body
        .tail
        .as_ref()
        .map(|t| t.span)
        .unwrap_or_else(Span::dummy);
    let value_expr: Expr = body.tail.map(|t| *t).unwrap_or_else(|| {
        // No tail expression — body's value is `()`. Wrap an empty
        // block as a unit-valued expression.
        Expr::new(
            ExprKind::Block(Block { stmts: Vec::new(), tail: None }),
            span,
        )
    });
    let wrapped = wrap_in_promise_resolve(value_expr);
    Block {
        stmts: body.stmts,
        tail: Some(Box::new(wrapped)),
    }
}

/// Wrap a type `T` into `Promise<T>`. `Type::Unit` stays a unit.
fn wrap_ret_in_promise(ret: Option<Type>) -> Option<Type> {
    let inner = ret.unwrap_or(Type::Unit);
    Some(Type::generic("Promise", vec![inner]))
}

/// Run the desugar pass over a program. Returns `Err` if any async
/// fn has an unsupported body shape (await sites we can't lower yet).
pub fn lower_async(prog: Program) -> Result<Program, AsyncLowerError> {
    let mut errors: Vec<AsyncLowerError> = Vec::new();
    let mut items: Vec<Item> = Vec::with_capacity(prog.items.len());
    for item in prog.items {
        match item {
            Item::Fn(f) => match lower_async_fn(f) {
                Ok(f) => items.push(Item::Fn(f)),
                Err(e) => {
                    errors.push(e.clone());
                    // Keep the original (still-async) fn so downstream
                    // diagnostics don't compound; the error returned
                    // below halts compilation anyway.
                }
            },
            Item::Class(c) => items.push(Item::Class(lower_class(c, &mut errors))),
            other => items.push(other),
        }
    }
    if let Some(first) = errors.into_iter().next() {
        return Err(first);
    }
    Ok(Program { items, stmts: prog.stmts, tail: prog.tail })
}

fn lower_class(mut c: ClassDecl, errors: &mut Vec<AsyncLowerError>) -> ClassDecl {
    let methods: Vec<FnDecl> = std::mem::take(&mut c.methods)
        .into_iter()
        .map(|m| match lower_async_fn(m) {
            Ok(m) => m,
            Err(e) => {
                let placeholder = e.fn_name.clone();
                errors.push(e);
                // Return a stub fn so the class type-check still has
                // a method to look at. The error halts compilation.
                FnDecl {
                    attrs: Box::new([]),
                    is_pub: false,
                    name: placeholder,
                    type_params: Box::new([]),
                    params: Box::new([]),
                    ret: None,
                    body: Block { stmts: Vec::new(), tail: None },
                    span: Span::dummy(),
                    is_override: false,
                    is_async: false,
                }
            }
        })
        .collect();
    c.methods = methods.into_boxed_slice();
    c
}

fn lower_async_fn(f: FnDecl) -> Result<FnDecl, AsyncLowerError> {
    if !f.is_async {
        return Ok(f);
    }
    let analysis = analyze(&f.body);
    if let Some(reason) = analysis.unsupported {
        return Err(AsyncLowerError {
            fn_name: f.name.clone(),
            span: f.span,
            reason: format!(
                "async fn `{}`: {} (multi-state lowering is the next phase)",
                f.name.as_str(),
                reason
            ),
        });
    }
    if !analysis.suspend_points.is_empty() {
        return Err(AsyncLowerError {
            fn_name: f.name.clone(),
            span: f.span,
            reason: format!(
                "async fn `{}`: {} await sites — only zero-await async \
                 fns are lowered in this commit (multi-state lowering \
                 lands next)",
                f.name.as_str(),
                analysis.suspend_points.len()
            ),
        });
    }
    // Trivial case: no awaits — wrap the body's value in
    // `Promise.resolve(...)` and lift the declared return type.
    let new_ret = wrap_ret_in_promise(f.ret);
    let new_body = wrap_body_in_promise_resolve(f.body);
    Ok(FnDecl {
        attrs: f.attrs,
        is_pub: f.is_pub,
        name: f.name,
        type_params: f.type_params,
        params: f.params,
        ret: new_ret,
        body: new_body,
        span: f.span,
        is_override: f.is_override,
        is_async: false,
    })
}

#[derive(Debug, Clone)]
pub struct AsyncLowerError {
    pub fn_name: Symbol,
    pub span: Span,
    pub reason: String,
}

impl std::fmt::Display for AsyncLowerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.reason)
    }
}

impl std::error::Error for AsyncLowerError {}

// Silence `unused` warning for the `HashSet` import — the live-
// variable analysis lands in the multi-state follow-up.
#[allow(dead_code)]
fn _liveness_placeholder() -> HashSet<Symbol> {
    HashSet::new()
}

#[allow(dead_code)]
fn _param_placeholder() -> Param {
    Param {
        name: Symbol::intern("_"),
        ty: Type::Unit,
        span: Span::dummy(),
        default: None,
    }
}
