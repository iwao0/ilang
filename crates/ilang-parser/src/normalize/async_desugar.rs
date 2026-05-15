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

use std::collections::{HashMap, HashSet};

use ilang_ast::{
    Block, ClassDecl, Expr, ExprKind, FieldDecl, FnDecl, Item, MatchArm, Param,
    Program, Span, Stmt, StmtKind, Symbol, Type,
};

/// One slot in the generated state struct. Slot 0 is the result
/// Promise pointer; slots 1.. mirror the live locals (params first,
/// then `let`-bindings in source order).
#[derive(Debug, Clone)]
pub struct StateSlot {
    /// Identifier the source body uses for this binding.
    pub name: Symbol,
    /// Declared (or inferred) type — used to size the cascade kind
    /// the runtime will release on poll completion.
    pub ty: Type,
    /// Byte offset within the state struct (0-based; the rc /
    /// state_idx header sits at offsets 0 and 8).
    pub offset: u32,
    /// Whether this slot was a function parameter (always live at
    /// every state) vs. an in-body `let` (live only after its
    /// introduction).
    pub from_param: bool,
}

/// One straight-line chunk of the async body, separated from the
/// previous chunk by an `await` suspend point. Chunk 0 runs from
/// the start of the body to the first await; chunk N runs from
/// the last await's continuation to the end.
#[derive(Debug, Clone)]
pub struct AwaitChunk {
    /// Statements in source order belonging to this chunk. The
    /// `let x = await p` that *terminates* the chunk is NOT part
    /// of it — instead, the chunk's `awaited_promise_expr` and
    /// `awaited_binding` capture that suspend point.
    pub stmts: Vec<Stmt>,
    /// `Some` if this chunk ends with `let name = await p` — the
    /// poll fn schedules `p`, then resumes the next chunk with
    /// `name` bound to p's resolved value. `None` for the final
    /// chunk (no trailing await; the body's tail expression
    /// becomes the result).
    pub awaited_promise_expr: Option<Expr>,
    pub awaited_binding: Option<Symbol>,
    /// `Some` only on the final chunk: the body's tail expression
    /// after every await has been processed. The poll fn settles
    /// the result Promise with this value.
    pub final_tail: Option<Expr>,
}

/// Result of analysing one async fn's body.
#[derive(Debug)]
pub struct AsyncAnalysis {
    /// Spans of every `await` expression we found.
    pub suspend_points: Vec<Span>,
    /// `Some(reason)` if the body shape isn't supported by the
    /// current lowering. `None` means we'll lower it.
    pub unsupported: Option<String>,
    /// State struct layout (poll fn reads / writes locals via
    /// these slot offsets). Empty for unsupported bodies.
    pub state_slots: Vec<StateSlot>,
    /// Body broken into per-state chunks. `chunks.len()` equals
    /// `suspend_points.len() + 1`. Empty for unsupported bodies.
    pub chunks: Vec<AwaitChunk>,
    /// Total bytes the state struct should be allocated at.
    /// `16 + 8 * state_slots.len()` (header + one i64 per slot).
    pub state_size: u32,
}

/// Walk an async fn body and collect its analysis. The `params`
/// argument supplies the parameter list (always live at every
/// suspend point); `ret` is the declared inner return type
/// (post-`async fn` unwrapping — i.e. `T` for `async fn foo(): T`).
pub fn analyze(params: &[Param], ret: &Type, body: &Block) -> AsyncAnalysis {
    let mut a = AsyncAnalysis {
        suspend_points: Vec::new(),
        unsupported: None,
        state_slots: Vec::new(),
        chunks: Vec::new(),
        state_size: 0,
    };
    walk_block(body, &mut a, /*top_level=*/ true);
    if a.unsupported.is_none() && !a.suspend_points.is_empty() {
        compute_slots_and_chunks(params, ret, body, &mut a);
    }
    a
}

/// Build the StateSlot table + AwaitChunk sequence for an async fn
/// body whose top-level statements are at most:
///   - sync `let / let-tuple / let-struct / Expr` statements
///   - `let name = await p` statements (the only suspend points
///     allowed by `walk_*`)
/// followed by an optional tail expression that doesn't itself
/// contain `await` (caught upstream by `walk_expr`'s "await in
/// sub-expression" check).
fn compute_slots_and_chunks(
    params: &[Param],
    _ret: &Type,
    body: &Block,
    a: &mut AsyncAnalysis,
) {
    // Header: rc @ 0, state_idx @ 8. Slot 0 = result Promise @ 16.
    let mut slots: Vec<StateSlot> = Vec::new();
    let mut next_off: u32 = 16; // state_idx is at +8, slot 0 starts here
    let promise_slot = StateSlot {
        name: Symbol::intern("__async_promise"),
        ty: Type::Object("Promise".into()),
        offset: next_off,
        from_param: false,
    };
    slots.push(promise_slot);
    next_off += 8;
    // Params first.
    for p in params {
        slots.push(StateSlot {
            name: p.name,
            ty: p.ty.clone(),
            offset: next_off,
            from_param: true,
        });
        next_off += 8;
    }
    // Then in-body let bindings, in source order. We don't compute
    // tight liveness — over-approximate by giving every let a slot.
    // The poll fn writes the slot at let-binding time and reads it
    // back when needed. Wasteful for short-lived locals but
    // correctness-preserving.
    let body_lets = collect_let_bindings(body);
    for (name, ty) in body_lets {
        slots.push(StateSlot {
            name,
            ty,
            offset: next_off,
            from_param: false,
        });
        next_off += 8;
    }
    a.state_size = next_off;
    a.state_slots = slots;

    // Build the chunks: split body.stmts on each `let _ = await p`.
    let mut chunks: Vec<AwaitChunk> = Vec::new();
    let mut cur: Vec<Stmt> = Vec::new();
    for s in &body.stmts {
        if let StmtKind::Let { name, value, .. } = &s.kind {
            if let ExprKind::Await(p) = &value.kind {
                chunks.push(AwaitChunk {
                    stmts: std::mem::take(&mut cur),
                    awaited_promise_expr: Some((**p).clone()),
                    awaited_binding: Some(*name),
                    final_tail: None,
                });
                continue;
            }
        }
        cur.push(s.clone());
    }
    // Final chunk: remaining stmts + the body's tail.
    chunks.push(AwaitChunk {
        stmts: cur,
        awaited_promise_expr: None,
        awaited_binding: None,
        final_tail: body.tail.as_deref().cloned(),
    });
    a.chunks = chunks;
}

fn collect_let_bindings(b: &Block) -> Vec<(Symbol, Type)> {
    let mut out: Vec<(Symbol, Type)> = Vec::new();
    let mut seen: HashSet<Symbol> = HashSet::new();
    for s in &b.stmts {
        if let StmtKind::Let { name, ty, value, .. } = &s.kind {
            if seen.insert(*name) {
                // Take the declared type when present; otherwise
                // record `Type::Unit` as a placeholder. The poll-fn
                // synth pass will infer from the value expression
                // when it walks the chunks.
                let _ = value;
                out.push((*name, ty.clone().unwrap_or(Type::Unit)));
            }
        }
    }
    out
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
///
/// An await-containing async fn expands into THREE items: a state
/// class, a poll fn, and the original-named wrapper that allocates
/// the state and the result promise. Zero-await async fns stay a
/// single fn (body wrapped in `Promise.resolve`).
pub fn lower_async(prog: Program) -> Result<Program, AsyncLowerError> {
    let mut errors: Vec<AsyncLowerError> = Vec::new();
    let mut items: Vec<Item> = Vec::with_capacity(prog.items.len());
    for item in prog.items {
        match item {
            Item::Fn(f) => match lower_async_fn(f) {
                Ok(AsyncLowerOutput::Single(f)) => items.push(Item::Fn(f)),
                Ok(AsyncLowerOutput::StateMachine {
                    wrapper,
                    state_class,
                    poll_fn,
                }) => {
                    items.push(Item::Class(state_class));
                    items.push(Item::Fn(poll_fn));
                    items.push(Item::Fn(wrapper));
                }
                Err(e) => {
                    errors.push(e.clone());
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

enum AsyncLowerOutput {
    Single(FnDecl),
    StateMachine {
        wrapper: FnDecl,
        state_class: ClassDecl,
        poll_fn: FnDecl,
    },
}

fn lower_class(mut c: ClassDecl, errors: &mut Vec<AsyncLowerError>) -> ClassDecl {
    let methods: Vec<FnDecl> = std::mem::take(&mut c.methods)
        .into_iter()
        .map(|m| match lower_async_fn(m) {
            Ok(AsyncLowerOutput::Single(f)) => f,
            Ok(AsyncLowerOutput::StateMachine { wrapper, .. }) => {
                // Class methods can't yet emit auxiliary top-level
                // items (the state class + poll fn), so we reject
                // multi-state lowering inside a class for this
                // iteration. The wrapper alone wouldn't work
                // because its body references the poll fn we'd
                // need to splice in.
                errors.push(AsyncLowerError {
                    fn_name: wrapper.name,
                    span: wrapper.span,
                    reason: "async methods inside a class can't carry \
                             await yet (the state class + poll fn \
                             would need to be lifted out next to the \
                             class itself); use a free `async fn` for now"
                        .into(),
                });
                FnDecl {
                    attrs: Box::new([]),
                    is_pub: false,
                    name: wrapper.name,
                    type_params: Box::new([]),
                    params: Box::new([]),
                    ret: None,
                    body: Block { stmts: Vec::new(), tail: None },
                    span: Span::dummy(),
                    is_override: false,
                    is_async: false,
                }
            }
            Err(e) => {
                let placeholder = e.fn_name.clone();
                errors.push(e);
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

fn lower_async_fn(f: FnDecl) -> Result<AsyncLowerOutput, AsyncLowerError> {
    if !f.is_async {
        return Ok(AsyncLowerOutput::Single(f));
    }
    let inner_ret = f.ret.clone().unwrap_or(Type::Unit);
    let analysis = analyze(&f.params, &inner_ret, &f.body);
    if let Some(reason) = analysis.unsupported {
        return Err(AsyncLowerError {
            fn_name: f.name.clone(),
            span: f.span,
            reason: format!(
                "async fn `{}`: {} (state-machine lowering doesn't \
                 cover this shape yet)",
                f.name.as_str(),
                reason
            ),
        });
    }
    // Zero-await: trivial wrap.
    if analysis.suspend_points.is_empty() {
        let new_ret = wrap_ret_in_promise(f.ret);
        let new_body = wrap_body_in_promise_resolve(f.body);
        return Ok(AsyncLowerOutput::Single(FnDecl {
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
        }));
    }
    // N awaits → state-machine: emit a state class, a poll fn, and
    // a wrapper fn that takes over the original name. Any number of
    // straight-line awaits is supported (the analyser already
    // rejected non-straight-line shapes via the `unsupported`
    // field).
    let (wrapper, state_class, poll_fn) = synthesize_state_machine(f, analysis)?;
    Ok(AsyncLowerOutput::StateMachine { wrapper, state_class, poll_fn })
}

// --------------------------------------------------------------------
// State-machine synthesis
//
// For
//     async fn foo(p: Promise<i64>, q: Promise<i64>): i64 {
//         let x: i64 = await p
//         let y: i64 = await q
//         x + y
//     }
//
// we emit:
//
//     class __foo_State {
//         pub state_idx: i64
//         pub __async_promise: Promise<i64>
//         pub p: Promise<i64>
//         pub q: Promise<i64>
//         pub x: i64
//         pub y: i64
//         pub init(p: Promise<i64>, q: Promise<i64>, prom: Promise<i64>) {
//             this.state_idx = 0
//             this.__async_promise = prom
//             this.p = p; this.q = q
//             this.x = 0; this.y = 0
//         }
//     }
//
//     fn __foo_poll(__state: __foo_State, __awaited_value: i64) {
//         if __state.state_idx == 0 {
//             // chunk 0 stmts (post-rewrite)
//             __state.state_idx = 1
//             let _d = __state.p.then(fn(v: i64): i64 {
//                 __foo_poll(__state, v); 0
//             })
//         } else if __state.state_idx == 1 {
//             __state.x = __awaited_value
//             // chunk 1 stmts
//             __state.state_idx = 2
//             let _d = __state.q.then(fn(v: i64): i64 {
//                 __foo_poll(__state, v); 0
//             })
//         } else {
//             __state.y = __awaited_value
//             // chunk 2 stmts + tail rewrite
//             let __result: i64 = __state.x + __state.y
//             Promise.__settleResolve(__state.__async_promise, __result)
//         }
//     }
//
//     fn foo(p: Promise<i64>, q: Promise<i64>): Promise<i64> {
//         let __async_prom: Promise<i64> = Promise.__pending()
//         let __async_state = new __foo_State(p, q, __async_prom)
//         __foo_poll(__async_state, 0)
//         __async_prom
//     }
// --------------------------------------------------------------------

fn mk_var(name: Symbol, span: Span) -> Expr {
    Expr::new(ExprKind::Var(name), span)
}
fn mk_int(n: i64, span: Span) -> Expr {
    Expr::new(ExprKind::Int(n), span)
}
fn mk_field(obj: Expr, name: Symbol, span: Span) -> Expr {
    Expr::new(ExprKind::Field { obj: Box::new(obj), name }, span)
}
fn mk_state_field(state_name: Symbol, field: Symbol, span: Span) -> Expr {
    mk_field(mk_var(state_name, span), field, span)
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
        StmtKind::Let {
            is_pub: false,
            is_const: false,
            name,
            ty,
            value,
        },
        span,
    )
}
fn mk_expr_stmt(e: Expr, span: Span) -> Stmt {
    Stmt::new(StmtKind::Expr(e), span)
}

/// Provide a literal default value for the type so init can pre-fill
/// post-await locals. Heap types use `0` (interpreted as a null
/// pointer); the field gets overwritten before any read at runtime,
/// so the null is never observed.
fn default_value_for(ty: &Type, span: Span) -> Expr {
    match ty {
        Type::F32 | Type::F64 => {
            Expr::new(ExprKind::Float(0.0), span)
        }
        Type::Bool => Expr::new(ExprKind::Bool(false), span),
        Type::Str => Expr::new(ExprKind::Str(String::new()), span),
        _ => mk_int(0, span),
    }
}

/// Walk the original body and collect every `let` binding's name +
/// type. Annotation is used when present; otherwise the mini-
/// inferencer below tries to derive the type from the RHS using
/// the param env + earlier let env. Returns `Err(name)` if a let
/// is un-annotated AND the inferencer can't handle its RHS shape.
fn collect_let_types(
    params: &[Param],
    b: &Block,
) -> Result<Vec<(Symbol, Type)>, Symbol> {
    let mut env: HashMap<Symbol, Type> = HashMap::new();
    for p in params {
        env.insert(p.name, p.ty.clone());
    }
    let mut out: Vec<(Symbol, Type)> = Vec::new();
    let mut seen: HashSet<Symbol> = HashSet::new();
    for s in &b.stmts {
        if let StmtKind::Let { name, ty, value, .. } = &s.kind {
            if !seen.insert(*name) {
                continue;
            }
            let t = if let Some(t) = ty {
                t.clone()
            } else {
                match infer_let_rhs(value, &env) {
                    Some(t) => t,
                    None => return Err(*name),
                }
            };
            env.insert(*name, t.clone());
            out.push((*name, t));
        }
    }
    Ok(out)
}

/// Best-effort type inference for the common let-RHS shapes the
/// state-machine desugar needs to recover when the user didn't
/// annotate. Returns `None` for shapes we can't decide locally
/// (the caller surfaces an actionable error pointing at the
/// missing annotation).
///
/// Supported:
/// - literals (int / float / bool / string)
/// - `Var(n)` where n is in scope
/// - `await E` where E's type can be inferred
/// - `Promise.resolve(arg)` (returns `Promise<typeof arg>`)
/// - `Promise.reject(...)` (returns `Promise<()>`)
/// - Simple arithmetic / comparison on i64 (returns i64 / bool)
fn infer_let_rhs(e: &Expr, env: &HashMap<Symbol, Type>) -> Option<Type> {
    match &e.kind {
        ExprKind::Int(_) => Some(Type::I64),
        ExprKind::Float(_) => Some(Type::F64),
        ExprKind::Bool(_) => Some(Type::Bool),
        ExprKind::Str(_) => Some(Type::Str),
        ExprKind::Var(n) => env.get(n).cloned(),
        ExprKind::Await(inner) => {
            let t = infer_let_rhs(inner, env)?;
            match t {
                Type::Generic(g)
                    if g.base.as_str() == "Promise" && g.args.len() == 1 =>
                {
                    Some(g.args[0].clone())
                }
                _ => None,
            }
        }
        ExprKind::MethodCall { obj, method, args } => {
            // `Promise.resolve(v)` / `Promise.reject(msg)` —
            // recognise the common static factories so a
            // `let p = Promise.resolve(...)` flows through.
            if let ExprKind::Var(n) = &obj.kind {
                if n.as_str() == "Promise" {
                    match method.as_str() {
                        "resolve" if args.len() == 1 => {
                            let inner = infer_let_rhs(&args[0], env)?;
                            return Some(Type::generic("Promise", vec![inner]));
                        }
                        "reject" => {
                            return Some(Type::generic("Promise", vec![Type::Unit]));
                        }
                        "__pending" => {
                            // Internal — returns `Promise<Any>` since
                            // the inner type can't be derived from
                            // args. Caller's binding annotation (if
                            // any) would override.
                            return Some(Type::generic("Promise", vec![Type::Any]));
                        }
                        _ => {}
                    }
                }
            }
            None
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let lt = infer_let_rhs(lhs, env)?;
            let rt = infer_let_rhs(rhs, env)?;
            use ilang_ast::BinOp;
            match op {
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                    Some(Type::Bool)
                }
                _ => {
                    if lt == rt {
                        Some(lt)
                    } else if matches!(lt, Type::I64) && matches!(rt, Type::I64) {
                        Some(Type::I64)
                    } else {
                        None
                    }
                }
            }
        }
        ExprKind::Unary { expr, .. } => infer_let_rhs(expr, env),
        _ => None,
    }
}

fn synthesize_state_machine(
    f: FnDecl,
    analysis: AsyncAnalysis,
) -> Result<(FnDecl, ClassDecl, FnDecl), AsyncLowerError> {
    let span = f.span;
    let inner_ret = f.ret.clone().unwrap_or(Type::Unit);
    let promise_ret = Type::generic("Promise", vec![inner_ret.clone()]);

    let state_class_name =
        Symbol::intern(&format!("__{}_State", f.name.as_str()));
    let poll_fn_name = Symbol::intern(&format!("__{}_poll", f.name.as_str()));
    let state_param = Symbol::intern("__state");
    let awaited_param = Symbol::intern("__awaited_value");

    // Every in-body let needs a known type so we can lay out the
    // state class. The mini-inferencer covers common RHS shapes
    // (literals, params, await on a typed Promise, Promise.resolve,
    // simple arithmetic); anything else still needs an explicit
    // annotation.
    let body_lets =
        collect_let_types(&f.params, &f.body).map_err(|missing| AsyncLowerError {
            fn_name: f.name.clone(),
            span: f.span,
            reason: format!(
                "async fn `{}`: `let {} = ...` — the state-machine \
                 desugar couldn't infer this binding's type from the \
                 RHS shape. Add an explicit `let {}: T = ...` \
                 annotation (the AST-stage desugar covers only a \
                 small subset of RHS shapes; full type inference \
                 runs after the desugar)",
                f.name.as_str(),
                missing.as_str(),
                missing.as_str()
            ),
        })?;

    // ---- State class -------------------------------------------------
    let mut fields: Vec<FieldDecl> = Vec::new();
    fields.push(FieldDecl {
        is_pub: true,
        name: Symbol::intern("state_idx"),
        ty: Type::I64,
        span,
        bits: None,
    });
    fields.push(FieldDecl {
        is_pub: true,
        name: Symbol::intern("__async_promise"),
        ty: promise_ret.clone(),
        span,
        bits: None,
    });
    for p in f.params.iter() {
        fields.push(FieldDecl {
            is_pub: true,
            name: p.name,
            ty: p.ty.clone(),
            span: p.span,
            bits: None,
        });
    }
    for (n, t) in &body_lets {
        fields.push(FieldDecl {
            is_pub: true,
            name: *n,
            ty: t.clone(),
            span,
            bits: None,
        });
    }

    let prom_init_param = Symbol::intern("__init_prom");
    let mut init_params: Vec<Param> = f.params.iter().cloned().collect();
    init_params.push(Param {
        name: prom_init_param,
        ty: promise_ret.clone(),
        span,
        default: None,
    });
    let this_e = || Expr::new(ExprKind::This, span);
    let mut init_stmts: Vec<Stmt> = Vec::new();
    init_stmts.push(mk_expr_stmt(
        mk_assign_field(this_e(), Symbol::intern("state_idx"), mk_int(0, span), span),
        span,
    ));
    init_stmts.push(mk_expr_stmt(
        mk_assign_field(
            this_e(),
            Symbol::intern("__async_promise"),
            mk_var(prom_init_param, span),
            span,
        ),
        span,
    ));
    for p in f.params.iter() {
        init_stmts.push(mk_expr_stmt(
            mk_assign_field(this_e(), p.name, mk_var(p.name, p.span), span),
            span,
        ));
    }
    for (n, t) in &body_lets {
        init_stmts.push(mk_expr_stmt(
            mk_assign_field(this_e(), *n, default_value_for(t, span), span),
            span,
        ));
    }
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
    let state_class = ClassDecl {
        extern_lib: None,
        is_repr_c: false,
        is_packed: false,
        is_union: false,
        is_pub: false,
        name: state_class_name,
        parent: None,
        interfaces: Box::new([]),
        type_params: Box::new([]),
        fields: fields.into_boxed_slice(),
        methods: Box::new([init_method]),
        static_methods: Box::new([]),
        static_fields: Box::new([]),
        properties: Box::new([]),
        span,
    };

    // ---- Poll fn -----------------------------------------------------
    // Names of all "body locals" we rewrite to state field accesses
    // inside chunk bodies.
    let mut state_locals: HashSet<Symbol> = HashSet::new();
    for p in f.params.iter() {
        state_locals.insert(p.name);
    }
    for (n, _) in &body_lets {
        state_locals.insert(*n);
    }

    let poll_body = build_poll_body(
        &analysis.chunks,
        state_param,
        awaited_param,
        poll_fn_name,
        &state_locals,
        &inner_ret,
        span,
    );

    let poll_fn = FnDecl {
        attrs: Box::new([]),
        is_pub: false,
        name: poll_fn_name,
        type_params: Box::new([]),
        params: Box::new([
            Param {
                name: state_param,
                ty: Type::Object(state_class_name),
                span,
                default: None,
            },
            Param {
                name: awaited_param,
                ty: Type::I64,
                span,
                default: None,
            },
        ]),
        ret: None,
        body: poll_body,
        span,
        is_override: false,
        is_async: false,
    };

    // ---- Wrapper fn (replaces the original) --------------------------
    let prom_local = Symbol::intern("__async_prom");
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
    let mut new_args: Vec<Expr> =
        f.params.iter().map(|p| mk_var(p.name, p.span)).collect();
    new_args.push(mk_var(prom_local, span));
    wrapper_stmts.push(mk_let(
        state_local,
        None,
        Expr::new(
            ExprKind::New {
                class: state_class_name,
                type_args: Box::new([]),
                args: new_args.into_boxed_slice(),
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
    let wrapper_fn = FnDecl {
        attrs: f.attrs.clone(),
        is_pub: f.is_pub,
        name: f.name,
        type_params: f.type_params.clone(),
        params: f.params.clone(),
        ret: Some(promise_ret),
        body: wrapper_body,
        span: f.span,
        is_override: f.is_override,
        is_async: false,
    };

    Ok((wrapper_fn, state_class, poll_fn))
}

/// Build the if-elif chain that switches on `state.state_idx`.
fn build_poll_body(
    chunks: &[AwaitChunk],
    state_param: Symbol,
    awaited_param: Symbol,
    poll_fn_name: Symbol,
    state_locals: &HashSet<Symbol>,
    inner_ret: &Type,
    span: Span,
) -> Block {
    // Each chunk K's branch produces a Block. We then nest them as
    // `if state.state_idx == 0 { B0 } else if ... { B_{N-1} } else { B_N }`.
    let n = chunks.len();
    let mut branch_blocks: Vec<Block> = Vec::with_capacity(n);
    for (i, chunk) in chunks.iter().enumerate() {
        let mut stmts: Vec<Stmt> = Vec::new();
        // After-await binding: state.<binding> = __awaited_value
        if i > 0 {
            if let Some(b) = chunks[i - 1].awaited_binding {
                stmts.push(mk_expr_stmt(
                    mk_assign_field(
                        mk_var(state_param, span),
                        b,
                        mk_var(awaited_param, span),
                        span,
                    ),
                    span,
                ));
            }
        }
        // The chunk's own (rewritten) stmts.
        for s in &chunk.stmts {
            let mut s = s.clone();
            rewrite_stmt_locals(&mut s, state_param, state_locals);
            stmts.push(s);
        }
        if i + 1 < n {
            // Non-final: advance state_idx, schedule next await.
            stmts.push(mk_expr_stmt(
                mk_assign_field(
                    mk_var(state_param, span),
                    Symbol::intern("state_idx"),
                    mk_int((i + 1) as i64, span),
                    span,
                ),
                span,
            ));
            let mut awaited =
                chunk.awaited_promise_expr.clone().expect("non-final chunk has await");
            rewrite_expr_locals(&mut awaited, state_param, state_locals);
            // Synthesize the continuation closure:
            //   fn(__v: i64): i64 { __${poll_fn_name}(__state, __v); 0 }
            let v_name = Symbol::intern("__v");
            let closure_body = Block {
                stmts: vec![mk_expr_stmt(
                    mk_call(
                        poll_fn_name,
                        vec![mk_var(state_param, span), mk_var(v_name, span)],
                        span,
                    ),
                    span,
                )],
                tail: Some(Box::new(mk_int(0, span))),
            };
            let closure = Expr::new(
                ExprKind::FnExpr {
                    params: Box::new([Param {
                        name: v_name,
                        ty: Type::I64,
                        span,
                        default: None,
                    }]),
                    ret: Some(Type::I64),
                    body: closure_body,
                },
                span,
            );
            let then_call = mk_method_call(
                awaited,
                Symbol::intern("then"),
                vec![closure],
                span,
            );
            stmts.push(mk_let(
                Symbol::intern("_"),
                None,
                then_call,
                span,
            ));
        } else {
            // Final chunk: compute the tail and settle the result
            // Promise. If no tail, settle with unit (we emit `0` for
            // the i64-shape ABI).
            let mut tail = chunk
                .final_tail
                .clone()
                .unwrap_or_else(|| mk_int(0, span));
            rewrite_expr_locals(&mut tail, state_param, state_locals);
            // Settle with the typed tail value.
            stmts.push(mk_expr_stmt(
                mk_method_call(
                    mk_var(Symbol::intern("Promise"), span),
                    Symbol::intern("__settleResolve"),
                    vec![
                        mk_state_field(
                            state_param,
                            Symbol::intern("__async_promise"),
                            span,
                        ),
                        tail,
                    ],
                    span,
                ),
                span,
            ));
            let _ = inner_ret; // currently unused but kept for future kind logic
        }
        branch_blocks.push(Block { stmts, tail: None });
    }
    // Nest the branches as if-elif-else.
    let mut else_branch: Option<Expr> = Some(Expr::new(
        ExprKind::Block(branch_blocks.pop().expect("at least one chunk")),
        span,
    ));
    for (idx, blk) in branch_blocks.into_iter().enumerate().rev() {
        let cond = Expr::new(
            ExprKind::Binary {
                op: ilang_ast::BinOp::Eq,
                lhs: Box::new(mk_state_field(
                    state_param,
                    Symbol::intern("state_idx"),
                    span,
                )),
                rhs: Box::new(mk_int(idx as i64, span)),
            },
            span,
        );
        let if_e = Expr::new(
            ExprKind::If {
                cond: Box::new(cond),
                then_branch: blk,
                else_branch: else_branch.map(Box::new),
            },
            span,
        );
        else_branch = Some(if_e);
    }
    Block {
        stmts: vec![mk_expr_stmt(else_branch.unwrap(), span)],
        tail: None,
    }
}

/// Rewrite `Var(name)` and `Assign(name, ..)` inside `e` to access
/// `state.name` when `name` is in `locals`. Recurses into every
/// child expression / block. The `Awaitable` form is left alone —
/// it never appears inside a chunk's stmts (awaits are extracted
/// to chunk boundaries).
fn rewrite_expr_locals(e: &mut Expr, state: Symbol, locals: &HashSet<Symbol>) {
    let span = e.span;
    match &mut e.kind {
        ExprKind::Var(n) if locals.contains(n) => {
            let name = *n;
            e.kind = ExprKind::Field {
                obj: Box::new(mk_var(state, span)),
                name,
            };
        }
        ExprKind::Var(_) => {}
        ExprKind::Assign { target, value } if locals.contains(target) => {
            rewrite_expr_locals(value, state, locals);
            let val = std::mem::replace(
                value,
                Box::new(Expr::new(ExprKind::None, span)),
            );
            let field = *target;
            e.kind = ExprKind::AssignField {
                obj: Box::new(mk_var(state, span)),
                field,
                value: val,
                is_init: false,
            };
        }
        ExprKind::Assign { value, .. } => rewrite_expr_locals(value, state, locals),
        ExprKind::AssignField { obj, value, .. }
        | ExprKind::AssignIndex { obj, value, .. } => {
            rewrite_expr_locals(obj, state, locals);
            rewrite_expr_locals(value, state, locals);
        }
        ExprKind::Block(b) => rewrite_block_locals(b, state, locals),
        ExprKind::If { cond, then_branch, else_branch } => {
            rewrite_expr_locals(cond, state, locals);
            rewrite_block_locals(then_branch, state, locals);
            if let Some(eb) = else_branch {
                rewrite_expr_locals(eb, state, locals);
            }
        }
        ExprKind::IfLet { expr, then_branch, else_branch, .. } => {
            rewrite_expr_locals(expr, state, locals);
            rewrite_block_locals(then_branch, state, locals);
            if let Some(eb) = else_branch {
                rewrite_expr_locals(eb, state, locals);
            }
        }
        ExprKind::While { cond, body } => {
            rewrite_expr_locals(cond, state, locals);
            rewrite_block_locals(body, state, locals);
        }
        ExprKind::Loop { body } => rewrite_block_locals(body, state, locals),
        ExprKind::ForIn { iter, body, .. } => {
            rewrite_expr_locals(iter, state, locals);
            rewrite_block_locals(body, state, locals);
        }
        ExprKind::Match { scrutinee, arms } => {
            rewrite_expr_locals(scrutinee, state, locals);
            for arm in arms.iter_mut() {
                rewrite_expr_locals(&mut arm.body, state, locals);
            }
            let _ = MatchArm { pattern: arms[0].pattern.clone(), body: arms[0].body.clone(), span: arms[0].span };
        }
        ExprKind::Call { args, .. }
        | ExprKind::SuperCall { args, .. }
        | ExprKind::New { args, .. } => {
            for a in args.iter_mut() {
                rewrite_expr_locals(a, state, locals);
            }
        }
        ExprKind::MethodCall { obj, args, .. } => {
            rewrite_expr_locals(obj, state, locals);
            for a in args.iter_mut() {
                rewrite_expr_locals(a, state, locals);
            }
        }
        ExprKind::Field { obj, .. } => rewrite_expr_locals(obj, state, locals),
        ExprKind::Index { obj, index } => {
            rewrite_expr_locals(obj, state, locals);
            rewrite_expr_locals(index, state, locals);
        }
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
            rewrite_expr_locals(lhs, state, locals);
            rewrite_expr_locals(rhs, state, locals);
        }
        ExprKind::Unary { expr, .. }
        | ExprKind::Cast { expr, .. }
        | ExprKind::TypeTest { expr, .. }
        | ExprKind::TypeDowncast { expr, .. } => {
            rewrite_expr_locals(expr, state, locals);
        }
        ExprKind::Some(inner) | ExprKind::Await(inner) => {
            rewrite_expr_locals(inner, state, locals);
        }
        ExprKind::Return(opt) | ExprKind::Break(opt) => {
            if let Some(e) = opt {
                rewrite_expr_locals(e, state, locals);
            }
        }
        ExprKind::FnExpr { body, .. } => {
            // Closure bodies *do* need rewriting: the continuation
            // closures the poll fn emits reference `state` directly
            // (already a free var the rewriter shouldn't touch since
            // `state` isn't in `locals`), but a user-written closure
            // inside a chunk would capture body locals — those need
            // to flow through state field accesses too.
            rewrite_block_locals(body, state, locals);
        }
        ExprKind::Tuple(es) | ExprKind::Array(es) => {
            for e in es.iter_mut() {
                rewrite_expr_locals(e, state, locals);
            }
        }
        ExprKind::MapLit(entries) => {
            for (k, v) in entries.iter_mut() {
                rewrite_expr_locals(k, state, locals);
                rewrite_expr_locals(v, state, locals);
            }
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(e) = start {
                rewrite_expr_locals(e, state, locals);
            }
            if let Some(e) = end {
                rewrite_expr_locals(e, state, locals);
            }
        }
        ExprKind::EnumCtor { args, .. } => match args {
            ilang_ast::CtorArgs::Unit => {}
            ilang_ast::CtorArgs::Tuple(es) => {
                for e in es.iter_mut() {
                    rewrite_expr_locals(e, state, locals);
                }
            }
            ilang_ast::CtorArgs::Struct(fs) => {
                for (_, e) in fs.iter_mut() {
                    rewrite_expr_locals(e, state, locals);
                }
            }
        },
        _ => {}
    }
}

fn rewrite_block_locals(b: &mut Block, state: Symbol, locals: &HashSet<Symbol>) {
    for s in b.stmts.iter_mut() {
        rewrite_stmt_locals(s, state, locals);
    }
    if let Some(t) = b.tail.as_mut() {
        rewrite_expr_locals(t, state, locals);
    }
}

fn rewrite_stmt_locals(s: &mut Stmt, state: Symbol, locals: &HashSet<Symbol>) {
    match &mut s.kind {
        StmtKind::Let { name, value, .. } => {
            // A *fresh* `let` inside a chunk introduces a new
            // local — and since `state_locals` is computed from
            // the original (pre-desugar) body, the freshly-
            // introduced name is already in the set if it was a
            // top-level body let. The rewriter then converts the
            // `let x = ...` introduction to `state.x = ...`.
            rewrite_expr_locals(value, state, locals);
            if locals.contains(name) {
                let n = *name;
                let v = value.clone();
                let span = s.span;
                s.kind = StmtKind::Expr(mk_assign_field(
                    mk_var(state, span),
                    n,
                    v,
                    span,
                ));
            }
        }
        StmtKind::LetTuple { value, .. } | StmtKind::LetStruct { value, .. } => {
            rewrite_expr_locals(value, state, locals);
        }
        StmtKind::Expr(e) => rewrite_expr_locals(e, state, locals),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_async_body(src: &str) -> FnDecl {
        let tokens = ilang_lexer::tokenize(src).expect("lex");
        let prog = crate::parse(&tokens).expect("parse");
        prog.items
            .into_iter()
            .find_map(|i| match i {
                Item::Fn(f) if f.is_async => Some(f),
                _ => None,
            })
            .expect("async fn")
    }

    #[test]
    fn analyzes_zero_await_body() {
        let f = parse_async_body("async fn foo(): i64 { 42 }");
        let inner = f.ret.clone().unwrap_or(Type::Unit);
        let a = analyze(&f.params, &inner, &f.body);
        assert!(a.suspend_points.is_empty());
        assert!(a.unsupported.is_none());
        // No suspend points → no chunks computed.
        assert!(a.chunks.is_empty());
        assert!(a.state_slots.is_empty());
    }

    #[test]
    fn analyzes_let_await_chain() {
        let f = parse_async_body(
            "async fn run(p: Promise<i64>, q: Promise<i64>): i64 {
                let x = await p
                let y = await q
                x + y
            }",
        );
        let inner = f.ret.clone().unwrap_or(Type::Unit);
        let a = analyze(&f.params, &inner, &f.body);
        assert_eq!(a.suspend_points.len(), 2);
        assert!(a.unsupported.is_none(), "got: {:?}", a.unsupported);
        // 3 chunks: pre-first-await, post-first-pre-second, final.
        assert_eq!(a.chunks.len(), 3);
        assert!(a.chunks[0].stmts.is_empty());
        assert_eq!(a.chunks[0].awaited_binding, Some(Symbol::intern("x")));
        assert_eq!(a.chunks[1].awaited_binding, Some(Symbol::intern("y")));
        assert!(a.chunks[2].awaited_promise_expr.is_none());
        assert!(a.chunks[2].final_tail.is_some());
        // State slots: promise + 2 params + 2 lets = 5 slots; 16-byte
        // header + 5 * 8 = 56 bytes total.
        assert_eq!(a.state_slots.len(), 5);
        assert_eq!(a.state_size, 16 + 5 * 8);
        assert_eq!(a.state_slots[0].name, Symbol::intern("__async_promise"));
        assert_eq!(a.state_slots[1].name, Symbol::intern("p"));
        assert_eq!(a.state_slots[1].from_param, true);
        assert_eq!(a.state_slots[3].name, Symbol::intern("x"));
        assert_eq!(a.state_slots[3].from_param, false);
    }

    #[test]
    fn rejects_await_in_subexpression() {
        let f = parse_async_body(
            "async fn run(p: Promise<i64>): i64 {
                let x = (await p) + 1
                x
            }",
        );
        let inner = f.ret.clone().unwrap_or(Type::Unit);
        let a = analyze(&f.params, &inner, &f.body);
        assert!(a.unsupported.is_some());
    }
}
