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
                 (the lifting pre-pass should have handled this; \
                 awaits in `&&` / `||` short-circuit rhs and inside \
                 nested closures aren't lifted)"
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
    // Collect every top-level fn's return type so the mini-
    // inferencer can recover `let a = await computeAsync(...)` —
    // it looks up `computeAsync`'s declared `: T` and tells the
    // synthesiser that `a` is `T`. Includes async fn returns
    // pre-wrapped to `Promise<T>` since callers see the wrapped
    // signature.
    let mut fn_returns: HashMap<Symbol, Type> = HashMap::new();
    for item in &prog.items {
        if let Item::Fn(f) = item {
            let ret = f.ret.clone().unwrap_or(Type::Unit);
            let ret = if f.is_async {
                Type::generic("Promise", vec![ret])
            } else {
                ret
            };
            fn_returns.insert(f.name, ret);
        }
    }
    let mut errors: Vec<AsyncLowerError> = Vec::new();
    let mut items: Vec<Item> = Vec::with_capacity(prog.items.len());
    for item in prog.items {
        match item {
            Item::Fn(f) => match lower_async_fn(f, &fn_returns) {
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
    let empty: HashMap<Symbol, Type> = HashMap::new();
    let methods: Vec<FnDecl> = std::mem::take(&mut c.methods)
        .into_iter()
        .map(|m| match lower_async_fn(m, &empty) {
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

fn lower_async_fn(
    mut f: FnDecl,
    fn_returns: &HashMap<Symbol, Type>,
) -> Result<AsyncLowerOutput, AsyncLowerError> {
    if !f.is_async {
        return Ok(AsyncLowerOutput::Single(f));
    }
    // Pre-pass: lift every `await E` that appears inside a sub-
    // expression into its own `let __await_tN = await E` statement
    // above the use site. The analyser + synthesiser only handle
    // the canonical "await as direct let RHS" form; this pass
    // makes `foo(await p, await q)` and `bar(await p) + 1` flow
    // through to the same lowering.
    f.body = lift_subexpr_awaits(f.body);
    let inner_ret = f.ret.clone().unwrap_or(Type::Unit);
    let analysis = analyze(&f.params, &inner_ret, &f.body);
    // The chunks-based analyser marks awaits inside `if` / `while`
    // / `match` as unsupported. For body-tail `if-else` we have the
    // BlockBuilder path that handles them properly, so skip the
    // unsupported check in that case (the BlockBuilder validates
    // sub-shapes as it walks).
    let block_builder_handles = body_has_control_flow_with_await(&f.body);
    if let Some(reason) = analysis.unsupported.clone() {
        if !block_builder_handles {
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
    }
    // Zero-await: trivial wrap.
    if analysis.suspend_points.is_empty() && !block_builder_handles {
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
    let (wrapper, state_class, poll_fn) =
        synthesize_state_machine(f, analysis, fn_returns)?;
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
    fn_returns: &HashMap<Symbol, Type>,
) -> Result<Vec<(Symbol, Type)>, Symbol> {
    let mut env: HashMap<Symbol, Type> = HashMap::new();
    for p in params {
        env.insert(p.name, p.ty.clone());
    }
    let mut out: Vec<(Symbol, Type)> = Vec::new();
    let mut seen: HashSet<Symbol> = HashSet::new();
    walk_block_for_lets(b, &mut env, &mut out, &mut seen, fn_returns)?;
    Ok(out)
}

/// Recursive helper: walks a block (and any if-else at tail
/// position) accumulating every `let` binding into the seen / out
/// tables. We DON'T descend into `if` arms unless the if appears at
/// a block's tail (the BlockBuilder only supports if-at-tail; mid-
/// body if-else still hits the analyser's nested-block rejection).
fn walk_block_for_lets(
    b: &Block,
    env: &mut HashMap<Symbol, Type>,
    out: &mut Vec<(Symbol, Type)>,
    seen: &mut HashSet<Symbol>,
    fn_returns: &HashMap<Symbol, Type>,
) -> Result<(), Symbol> {
    for s in &b.stmts {
        if let StmtKind::Let { name, ty, value, .. } = &s.kind {
            if !seen.insert(*name) {
                continue;
            }
            let t = if let Some(t) = ty {
                t.clone()
            } else {
                match infer_let_rhs(value, env, fn_returns) {
                    Some(t) => t,
                    None => return Err(*name),
                }
            };
            env.insert(*name, t.clone());
            out.push((*name, t));
        }
    }
    if let Some(tail) = &b.tail {
        walk_if_tail_for_lets(tail, env, out, seen, fn_returns)?;
    }
    Ok(())
}

fn walk_if_tail_for_lets(
    e: &Expr,
    env: &mut HashMap<Symbol, Type>,
    out: &mut Vec<(Symbol, Type)>,
    seen: &mut HashSet<Symbol>,
    fn_returns: &HashMap<Symbol, Type>,
) -> Result<(), Symbol> {
    match &e.kind {
        ExprKind::If { then_branch, else_branch, .. } => {
            walk_block_for_lets(then_branch, env, out, seen, fn_returns)?;
            if let Some(eb) = else_branch {
                walk_if_tail_for_lets(eb, env, out, seen, fn_returns)?;
            }
        }
        ExprKind::Block(b) => {
            walk_block_for_lets(b, env, out, seen, fn_returns)?;
        }
        _ => {}
    }
    Ok(())
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
fn infer_let_rhs(
    e: &Expr,
    env: &HashMap<Symbol, Type>,
    fn_returns: &HashMap<Symbol, Type>,
) -> Option<Type> {
    match &e.kind {
        ExprKind::Int(_) => Some(Type::I64),
        ExprKind::Float(_) => Some(Type::F64),
        ExprKind::Bool(_) => Some(Type::Bool),
        ExprKind::Str(_) => Some(Type::Str),
        ExprKind::Var(n) => env.get(n).cloned(),
        ExprKind::Call { callee, .. } => fn_returns.get(callee).cloned(),
        ExprKind::Await(inner) => {
            let t = infer_let_rhs(inner, env, fn_returns)?;
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
                            let inner = infer_let_rhs(&args[0], env, fn_returns)?;
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
            let lt = infer_let_rhs(lhs, env, fn_returns)?;
            let rt = infer_let_rhs(rhs, env, fn_returns)?;
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
        ExprKind::Unary { expr, .. } => infer_let_rhs(expr, env, fn_returns),
        _ => None,
    }
}

fn synthesize_state_machine(
    f: FnDecl,
    analysis: AsyncAnalysis,
    fn_returns: &HashMap<Symbol, Type>,
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
        collect_let_types(&f.params, &f.body, fn_returns).map_err(|missing| AsyncLowerError {
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

    // Pick the lowering path: control-flow-aware BlockBuilder for
    // bodies with `if-else` at tail; chunks-based path otherwise.
    let poll_body = if body_has_control_flow_with_await(&f.body) {
        let blocks = build_blocks_for_body(
            &f.body,
            state_param,
            awaited_param,
            &state_locals,
            span,
        )
        .map_err(|reason| AsyncLowerError {
            fn_name: f.name,
            span: f.span,
            reason: format!("async fn `{}`: {}", f.name.as_str(), reason),
        })?;
        build_poll_body_from_blocks(&blocks, state_param, poll_fn_name, span)
    } else {
        build_poll_body(
            &analysis.chunks,
            state_param,
            awaited_param,
            poll_fn_name,
            &state_locals,
            &inner_ret,
            span,
        )
    };

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

/// One basic block of the poll fn's CFG. `idx` is the state_idx
/// value that selects this block; `stmts` are the bound-rewritten
/// statements to run on entry; `terminator` is what the block does
/// at the end (suspend / settle / jump / branch).
#[derive(Debug, Clone)]
pub struct StateBlock {
    pub idx: u32,
    pub stmts: Vec<Stmt>,
    pub terminator: BlockTerminator,
}

#[derive(Debug, Clone)]
pub enum BlockTerminator {
    /// `let _ = <promise>.then(fn(__v) { __poll(state, __v); 0 })`
    /// then `return`. The runtime calls back into the poll fn with
    /// the resolved value once the promise settles; the driver
    /// loop's first step is to `state.state_idx = resume_idx`
    /// (which happens just BEFORE the `.then` registration, so the
    /// continuation runs against the right resume state).
    Suspend { promise: Expr, resume_idx: u32, binding: Option<Symbol> },
    /// `Promise.__settleResolve(state.__async_promise, value); return`.
    Settle { value: Expr },
    /// `state.state_idx = N; continue` — re-enters the driver loop
    /// at the top.
    #[allow(dead_code)]
    Jump(u32),
    /// `if cond { state.state_idx = then_idx; continue }
    ///  else { state.state_idx = else_idx; continue }`.
    #[allow(dead_code)]
    Branch { cond: Expr, then_idx: u32, else_idx: u32 },
}

/// Translate the analyser's straight-line `AwaitChunk` vec into the
/// generic CFG form. For each chunk K:
///   - Block K's stmts: optionally a binding assign for the prior
///     await's resolved value, then the chunk's own stmts.
///   - Block K's terminator: `Suspend` (non-final chunk) or
///     `Settle` (final chunk).
fn chunks_to_blocks(
    chunks: &[AwaitChunk],
    state_param: Symbol,
    awaited_param: Symbol,
    state_locals: &HashSet<Symbol>,
    span: Span,
) -> Vec<StateBlock> {
    let n = chunks.len();
    let mut blocks: Vec<StateBlock> = Vec::with_capacity(n);
    for (i, chunk) in chunks.iter().enumerate() {
        let mut stmts: Vec<Stmt> = Vec::new();
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
        for s in &chunk.stmts {
            let mut s = s.clone();
            rewrite_stmt_locals(&mut s, state_param, state_locals);
            stmts.push(s);
        }
        let terminator = if i + 1 < n {
            let mut awaited =
                chunk.awaited_promise_expr.clone().expect("non-final chunk has await");
            rewrite_expr_locals(&mut awaited, state_param, state_locals);
            BlockTerminator::Suspend {
                promise: awaited,
                resume_idx: (i + 1) as u32,
                binding: chunk.awaited_binding,
            }
        } else {
            let mut tail = chunk
                .final_tail
                .clone()
                .unwrap_or_else(|| mk_int(0, span));
            rewrite_expr_locals(&mut tail, state_param, state_locals);
            BlockTerminator::Settle { value: tail }
        };
        blocks.push(StateBlock {
            idx: i as u32,
            stmts,
            terminator,
        });
    }
    blocks
}

/// Emit one state block's body — its setup stmts plus the
/// terminator-driven trailing stmts that either return (Suspend /
/// Settle) or fall through to the loop top (Jump / Branch).
fn emit_block_body(
    block: &StateBlock,
    state_param: Symbol,
    poll_fn_name: Symbol,
    span: Span,
) -> Block {
    let mut stmts: Vec<Stmt> = block.stmts.clone();
    match &block.terminator {
        BlockTerminator::Suspend { promise, resume_idx, binding: _ } => {
            // Advance state_idx BEFORE scheduling the continuation
            // (the continuation closure captures `state` and re-
            // enters poll, which dispatches on state_idx).
            stmts.push(mk_expr_stmt(
                mk_assign_field(
                    mk_var(state_param, span),
                    Symbol::intern("state_idx"),
                    mk_int(*resume_idx as i64, span),
                    span,
                ),
                span,
            ));
            // Continuation closure: `fn(__v: i64): i64 { poll(state, __v); 0 }`
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
                promise.clone(),
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
        BlockTerminator::Settle { value } => {
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
                        value.clone(),
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
        BlockTerminator::Jump(n) => {
            stmts.push(mk_expr_stmt(
                mk_assign_field(
                    mk_var(state_param, span),
                    Symbol::intern("state_idx"),
                    mk_int(*n as i64, span),
                    span,
                ),
                span,
            ));
            stmts.push(mk_expr_stmt(
                Expr::new(ExprKind::Continue, span),
                span,
            ));
        }
        BlockTerminator::Branch { cond, then_idx, else_idx } => {
            // `if cond { state.state_idx = T; continue } else { ...F... }`.
            let then_blk = Block {
                stmts: vec![
                    mk_expr_stmt(
                        mk_assign_field(
                            mk_var(state_param, span),
                            Symbol::intern("state_idx"),
                            mk_int(*then_idx as i64, span),
                            span,
                        ),
                        span,
                    ),
                    mk_expr_stmt(Expr::new(ExprKind::Continue, span), span),
                ],
                tail: None,
            };
            let else_blk_expr = Expr::new(
                ExprKind::Block(Block {
                    stmts: vec![
                        mk_expr_stmt(
                            mk_assign_field(
                                mk_var(state_param, span),
                                Symbol::intern("state_idx"),
                                mk_int(*else_idx as i64, span),
                                span,
                            ),
                            span,
                        ),
                        mk_expr_stmt(Expr::new(ExprKind::Continue, span), span),
                    ],
                    tail: None,
                }),
                span,
            );
            stmts.push(mk_expr_stmt(
                Expr::new(
                    ExprKind::If {
                        cond: Box::new(cond.clone()),
                        then_branch: then_blk,
                        else_branch: Some(Box::new(else_blk_expr)),
                    },
                    span,
                ),
                span,
            ));
        }
    }
    Block { stmts, tail: None }
}

/// Build the poll fn's body: a `loop { ... }` whose body is an
/// if-elif-else chain over `state.state_idx`, each branch ending
/// in a Suspend / Settle / Jump / Branch terminator (the first two
/// `return` from poll; the second two `continue` the loop).
fn build_poll_body_from_blocks(
    blocks: &[StateBlock],
    state_param: Symbol,
    poll_fn_name: Symbol,
    span: Span,
) -> Block {
    // Build the if-elif-else chain. Each branch is one StateBlock.
    let mut blocks_rev: Vec<&StateBlock> = blocks.iter().collect();
    // Final else: unreachable, return from poll.
    let mut else_branch: Option<Expr> = Some(Expr::new(
        ExprKind::Block(Block {
            stmts: vec![mk_expr_stmt(
                Expr::new(ExprKind::Return(None), span),
                span,
            )],
            tail: None,
        }),
        span,
    ));
    while let Some(blk) = blocks_rev.pop() {
        let body = emit_block_body(blk, state_param, poll_fn_name, span);
        let cond = Expr::new(
            ExprKind::Binary {
                op: ilang_ast::BinOp::Eq,
                lhs: Box::new(mk_state_field(
                    state_param,
                    Symbol::intern("state_idx"),
                    span,
                )),
                rhs: Box::new(mk_int(blk.idx as i64, span)),
            },
            span,
        );
        let if_e = Expr::new(
            ExprKind::If {
                cond: Box::new(cond),
                then_branch: body,
                else_branch: else_branch.map(Box::new),
            },
            span,
        );
        else_branch = Some(if_e);
    }
    // Wrap the if-elif-else in a `loop { ... }` so Jump / Branch
    // can `continue` back to the dispatch top. Suspend / Settle
    // `return` out of the fn, breaking the loop indirectly.
    let switch_expr = else_branch.unwrap();
    let loop_body = Block {
        stmts: vec![mk_expr_stmt(switch_expr, span)],
        tail: None,
    };
    let loop_expr = Expr::new(
        ExprKind::Loop { body: loop_body },
        span,
    );
    Block {
        stmts: vec![mk_expr_stmt(loop_expr, span)],
        tail: None,
    }
}

/// Convenience wrapper used by `synthesize_state_machine`:
/// translates straight-line chunks to blocks, then emits the
/// loop+switch body. The split lets future passes (if-else, while,
/// match) build blocks directly without going through chunks.
fn build_poll_body(
    chunks: &[AwaitChunk],
    state_param: Symbol,
    awaited_param: Symbol,
    poll_fn_name: Symbol,
    state_locals: &HashSet<Symbol>,
    inner_ret: &Type,
    span: Span,
) -> Block {
    let _ = inner_ret; // reserved for future per-state kind logic
    let blocks =
        chunks_to_blocks(chunks, state_param, awaited_param, state_locals, span);
    build_poll_body_from_blocks(&blocks, state_param, poll_fn_name, span)
}

// --------------------------------------------------------------------
// BlockBuilder — direct AST→StateBlock construction with `if`-tail
// branching support. Phase 2 of the CFG-based lowering.
//
// The chunks-based analyser walks the body once and produces a flat
// Vec<AwaitChunk> that assumes straight-line code. The BlockBuilder
// is a recursive walker that handles `if-else` at body-tail position
// by emitting Branch terminators and recursing into each arm.
//
// Supported shapes:
//   - Straight-line stmts (mirroring the chunks-based path).
//   - `let x = await E` suspend points.
//   - Body tail = `if cond { arm_a } else { arm_b }`. Each arm is
//     recursively built; both arms must settle the result promise
//     (no join — the if-else IS the body's value).
//   - Nested if-else within an arm's tail.
//
// Not yet:
//   - `while` / `match` / mid-body if-else (need a join state).
// --------------------------------------------------------------------

struct BlockBuilder<'a> {
    blocks: Vec<StateBlock>,
    next_idx: u32,
    state_param: Symbol,
    awaited_param: Symbol,
    state_locals: &'a HashSet<Symbol>,
    span: Span,
    /// Set when an unsupported shape is encountered during the build.
    /// The caller (lower_async_fn) surfaces it as an AsyncLowerError.
    error: Option<String>,
}

impl<'a> BlockBuilder<'a> {
    fn fresh_idx(&mut self) -> u32 {
        let i = self.next_idx;
        self.next_idx += 1;
        i
    }

    /// Build the state blocks corresponding to `body`. The first
    /// block carries `entry_idx` (assigned by the caller, so chained
    /// builders can reserve indices). `prior_binding` is set when
    /// `entry_idx`'s block resumes from a previous suspend point —
    /// the prior await's resolved value lives in `__awaited_value`
    /// and gets assigned to `state.<binding>` as the block's first
    /// statement.
    fn build_body(
        &mut self,
        body: &Block,
        entry_idx: u32,
        prior_binding: Option<Symbol>,
    ) {
        let span = self.span;
        let mut cur_idx = entry_idx;
        let mut cur_stmts: Vec<Stmt> = Vec::new();
        if let Some(b) = prior_binding {
            cur_stmts.push(mk_expr_stmt(
                mk_assign_field(
                    mk_var(self.state_param, span),
                    b,
                    mk_var(self.awaited_param, span),
                    span,
                ),
                span,
            ));
        }
        for stmt in &body.stmts {
            // Recognise `let x = await E` as a suspend point.
            if let StmtKind::Let { name, value, .. } = &stmt.kind {
                if let ExprKind::Await(p) = &value.kind {
                    let mut promise_expr = (**p).clone();
                    rewrite_expr_locals(
                        &mut promise_expr,
                        self.state_param,
                        self.state_locals,
                    );
                    let next_idx = self.fresh_idx();
                    self.blocks.push(StateBlock {
                        idx: cur_idx,
                        stmts: std::mem::take(&mut cur_stmts),
                        terminator: BlockTerminator::Suspend {
                            promise: promise_expr,
                            resume_idx: next_idx,
                            binding: Some(*name),
                        },
                    });
                    cur_idx = next_idx;
                    cur_stmts.push(mk_expr_stmt(
                        mk_assign_field(
                            mk_var(self.state_param, span),
                            *name,
                            mk_var(self.awaited_param, span),
                            span,
                        ),
                        span,
                    ));
                    continue;
                }
            }
            // Sync stmt: rewrite locals and accumulate.
            let mut s = stmt.clone();
            rewrite_stmt_locals(&mut s, self.state_param, self.state_locals);
            cur_stmts.push(s);
        }
        // Handle the tail. Three cases: None (settle unit), If
        // (branch + recurse into arms), or any other expr (settle).
        let tail = body.tail.as_deref();
        match tail.map(|t| (&t.kind, t.span)) {
            None => {
                self.blocks.push(StateBlock {
                    idx: cur_idx,
                    stmts: cur_stmts,
                    terminator: BlockTerminator::Settle {
                        value: mk_int(0, span),
                    },
                });
            }
            Some((ExprKind::If { cond, then_branch, else_branch }, _)) => {
                let mut cond_expr = (**cond).clone();
                rewrite_expr_locals(
                    &mut cond_expr,
                    self.state_param,
                    self.state_locals,
                );
                let then_idx = self.fresh_idx();
                let else_idx = self.fresh_idx();
                self.blocks.push(StateBlock {
                    idx: cur_idx,
                    stmts: cur_stmts,
                    terminator: BlockTerminator::Branch {
                        cond: cond_expr,
                        then_idx,
                        else_idx,
                    },
                });
                self.build_body(then_branch, then_idx, None);
                match else_branch {
                    Some(eb) => match &eb.kind {
                        ExprKind::Block(b) => self.build_body(b, else_idx, None),
                        // `else if ...` — wrap the nested If as the
                        // tail of a single-element synthetic block
                        // and recurse.
                        _ => {
                            let synth = Block {
                                stmts: Vec::new(),
                                tail: Some(Box::new((**eb).clone())),
                            };
                            self.build_body(&synth, else_idx, None);
                        }
                    },
                    None => {
                        // No else: this arm settles with unit.
                        self.blocks.push(StateBlock {
                            idx: else_idx,
                            stmts: Vec::new(),
                            terminator: BlockTerminator::Settle {
                                value: mk_int(0, span),
                            },
                        });
                    }
                }
            }
            Some((_, _)) => {
                // Regular tail expression: settle with its value.
                let mut tail_expr = body.tail.as_deref().unwrap().clone();
                rewrite_expr_locals(
                    &mut tail_expr,
                    self.state_param,
                    self.state_locals,
                );
                self.blocks.push(StateBlock {
                    idx: cur_idx,
                    stmts: cur_stmts,
                    terminator: BlockTerminator::Settle { value: tail_expr },
                });
            }
        }
    }
}

/// Top-level entry used by `synthesize_state_machine` when the
/// body contains control-flow constructs the chunks-based analyser
/// can't lay out. Returns `Err(reason)` if the BlockBuilder hit an
/// unsupported sub-shape.
fn build_blocks_for_body(
    body: &Block,
    state_param: Symbol,
    awaited_param: Symbol,
    state_locals: &HashSet<Symbol>,
    span: Span,
) -> Result<Vec<StateBlock>, String> {
    let mut b = BlockBuilder {
        blocks: Vec::new(),
        next_idx: 1,
        state_param,
        awaited_param,
        state_locals,
        span,
        error: None,
    };
    b.build_body(body, 0, None);
    if let Some(e) = b.error {
        return Err(e);
    }
    Ok(b.blocks)
}

/// Whether the body needs the BlockBuilder path (contains an `if`
/// at tail position with awaits inside) vs. the simpler chunks-
/// based path.
fn body_has_control_flow_with_await(body: &Block) -> bool {
    matches!(body.tail.as_deref().map(|t| &t.kind), Some(ExprKind::If { .. }))
        && body_or_arms_have_await(body)
}

fn body_or_arms_have_await(b: &Block) -> bool {
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
                || body_or_arms_have_await(then_branch)
                || else_branch.as_deref().is_some_and(expr_has_await)
        }
        ExprKind::Block(b) => body_or_arms_have_await(b),
        _ => false,
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
// --------------------------------------------------------------------
// Sub-expression `await` lifting
//
// Before the analyser / synthesiser run, we normalise the async fn
// body so that every `await E` appears as the direct RHS of a let
// (or, for the body's tail, gets pulled into one above the tail).
// `foo(await p, await q)` becomes
//     let __await_t0 = await p
//     let __await_t1 = await q
//     foo(__await_t0, __await_t1)
//
// We only lift awaits that appear at the *top-level* expression
// stack of a statement — awaits inside an `if` / `while` / `match` /
// closure body are left in place, and the analyser will still
// reject them as "nested-block await" (a separate follow-up).
// --------------------------------------------------------------------

fn lift_subexpr_awaits(body: Block) -> Block {
    let mut counter: u64 = 0;
    let mut new_stmts: Vec<Stmt> = Vec::new();
    for s in body.stmts {
        lift_stmt(s, &mut counter, &mut new_stmts);
    }
    let new_tail = body.tail.map(|t| {
        let span = t.span;
        let mut lifts: Vec<Stmt> = Vec::new();
        // The tail is the body's value. If it's a bare `await E`,
        // we still lift it (the synthesiser expects the final tail
        // to be a sync expression — the let-await becomes the
        // body's last suspend point and the tail reduces to
        // `__await_tN`).
        let tail_lifted = lift_in_expr(*t, &mut lifts, &mut counter, /*at_let_rhs=*/ false);
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
        // Everything that introduces a new scope (blocks, control
        // flow, closures) is NOT descended into. Awaits inside are
        // currently rejected by the analyser; lifting them out
        // would change observable order (e.g. an await inside one
        // `if` arm would run unconditionally).
        kind => Expr::new(kind, span),
    }
}

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
