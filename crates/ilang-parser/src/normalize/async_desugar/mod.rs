//! `async fn` → state-machine lowering.
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
//! ## Module layout
//!
//! - [`mod.rs`](self) — entry points (`lower_async`, `lower_class`,
//!   `lower_async_fn`), Promise-wrapping helpers, the
//!   `AsyncLowerError` type.
//! - [`await_scan`] — recursive walk that answers "does any `await`
//!   appear anywhere in this block?" Used to pick between the
//!   trivial wrap and the state-machine lowering.
//! - [`let_infer`] — mini type-inferencer for un-annotated `let`s
//!   inside async bodies. Necessary because the state enum's
//!   per-variant fields are typed from these recovered types.
//! - [`await_lift`] — pre-pass that hoists sub-expression `await`s
//!   into their own `let __await_tN = await E` statement so the
//!   state-machine builder only sees the canonical shape.
//!
//! The actual state-machine synthesis lives one level up in
//! [`crate::normalize::state_machine`].

use std::collections::HashMap;

use ilang_ast::{
    Block, ClassDecl, EnumDecl, Expr, ExprKind, FnDecl, Item, Program, Span, Symbol, Type,
};

use super::state_machine;

mod await_lift;
mod await_scan;
mod let_infer;

use await_lift::lift_subexpr_awaits;
use await_scan::body_contains_await;
use let_infer::{collect_let_types, stamp_inferred_let_types_block};

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

/// Wrap a type `T` into `Promise<T>`. If `T` is already
/// `Promise<U>` — i.e. the user wrote `async fn foo(): Promise<U>`
/// explicitly — leave it as is to avoid producing `Promise<Promise<U>>`.
fn wrap_ret_in_promise(ret: Option<Type>) -> Option<Type> {
    let inner = ret.unwrap_or(Type::Unit);
    if let Type::Generic(g) = &inner {
        if g.base.as_str() == "Promise" {
            return Some(inner);
        }
    }
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
    let mut enums: HashMap<Symbol, EnumDecl> = HashMap::new();
    let mut classes: HashMap<Symbol, ClassDecl> = HashMap::new();
    for item in &prog.items {
        if let Item::Fn(f) = item {
            let ret = f.ret.clone().unwrap_or(Type::Unit);
            // `async fn` callers see the Promise-wrapped signature.
            // If the user already wrote `Promise<U>` as the return
            // type, the desugar leaves it as-is; otherwise we wrap.
            let ret = if f.is_async {
                let already_promise = matches!(
                    &ret,
                    Type::Generic(g) if g.base.as_str() == "Promise"
                );
                if already_promise {
                    ret
                } else {
                    Type::generic("Promise", vec![ret])
                }
            } else {
                ret
            };
            fn_returns.insert(f.name, ret);
        }
        if let Item::Enum(e) = item {
            enums.insert(e.name, e.clone());
        }
        if let Item::Class(c) = item {
            classes.insert(c.name, c.clone());
        }
    }
    let mut errors: Vec<AsyncLowerError> = Vec::new();
    let mut items: Vec<Item> = Vec::with_capacity(prog.items.len());
    for item in prog.items {
        match item {
            Item::Fn(f) => match lower_async_fn(f, &fn_returns, None, &enums, &classes) {
                Ok(AsyncLowerOutput::Single(f)) => items.push(Item::Fn(f)),
                Ok(AsyncLowerOutput::StateMachine {
                    wrapper,
                    state_class,
                    poll_fn,
                    state_enum,
                }) => {
                    if let Some(e) = state_enum {
                        items.push(Item::Enum(e));
                    }
                    items.push(Item::Class(state_class));
                    items.push(Item::Fn(poll_fn));
                    items.push(Item::Fn(wrapper));
                }
                Err(e) => {
                    errors.push(e.clone());
                }
            },
            Item::Class(c) => {
                let lowered = lower_class(c, &mut items, &mut errors, &fn_returns, &enums, &classes);
                items.push(Item::Class(lowered));
            }
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
        /// Enum produced by the v2 (enum-variant) lowering. `None`
        /// means the legacy class-based path was used (state lives in
        /// `state_class` directly with all fields side-by-side).
        state_enum: Option<EnumDecl>,
    },
}

fn lower_class(
    mut c: ClassDecl,
    items: &mut Vec<Item>,
    errors: &mut Vec<AsyncLowerError>,
    fn_returns: &HashMap<Symbol, Type>,
    enums: &HashMap<Symbol, EnumDecl>,
    classes: &HashMap<Symbol, ClassDecl>,
) -> ClassDecl {
    let class_name = c.name;
    let methods: Vec<FnDecl> = std::mem::take(&mut c.methods)
        .into_iter()
        .map(|m| match lower_async_fn(m, fn_returns, Some(class_name), enums, classes) {
            Ok(AsyncLowerOutput::Single(f)) => f,
            Ok(AsyncLowerOutput::StateMachine {
                wrapper,
                state_class,
                poll_fn,
                state_enum,
            }) => {
                // Lift the auxiliary items out next to the
                // containing class. The wrapper stays as the
                // class method (now non-async, returns a Promise).
                if let Some(e) = state_enum {
                    items.push(Item::Enum(e));
                }
                items.push(Item::Class(state_class));
                items.push(Item::Fn(poll_fn));
                wrapper
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
                    intrinsic_name: None,
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
    enclosing_class: Option<Symbol>,
    enums: &HashMap<Symbol, EnumDecl>,
    classes: &HashMap<Symbol, ClassDecl>,
) -> Result<AsyncLowerOutput, AsyncLowerError> {
    if !f.is_async {
        return Ok(AsyncLowerOutput::Single(f));
    }
    // Pre-passes (run before lift_subexpr_awaits so that any awaits
    // introduced by the rewrites flow through the canonicaliser):
    //   0. `loop { ...await... }` → `while true { ... }`.
    f.body = state_machine::desugar_loop_to_while(f.body);
    //   1. `while await cond { ... }` → `while true { let cv = await cond; if !cv { break } ... }`
    f.body = state_machine::desugar_while_cond_await(f.body);
    // Canonicalise: every `await E` inside a sub-expression becomes
    // its own `let __await_tN = await E` statement above the use
    // site, so the state-machine builder only sees the "await as
    // direct let RHS" shape.
    f.body = lift_subexpr_awaits(f.body);
    //   2. `for v in s..e { ...await... }` → equivalent while loop.
    //      (Runs AFTER lift since for-in's iter rarely needs lifting.)
    f.body = state_machine::desugar_for_in_with_await(f.body);

    // Zero-await: trivial wrap into Promise.resolve(<body>).
    // If the user already declared the return type as `Promise<U>`,
    // the body's tail is already a `Promise<U>` value, so skip the
    // `Promise.resolve(...)` wrap (otherwise we'd build
    // `Promise<Promise<U>>`).
    if !body_contains_await(&f.body) {
        let ret_is_promise = matches!(
            &f.ret,
            Some(Type::Generic(g)) if g.base.as_str() == "Promise"
        );
        let new_ret = wrap_ret_in_promise(f.ret);
        let new_body = if ret_is_promise {
            f.body
        } else {
            wrap_body_in_promise_resolve(f.body)
        };
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
            intrinsic_name: None,
        }));
    }

    // ≥1 await: lower to enum-variant state machine via v2.
    //
    // After type inference, stamp every desugar-inferred type back
    // onto the originating `let` statement (when the user omitted
    // the annotation). Reason: the state enum's variant field is
    // typed from `body_lets`, and the segment builder also reads
    // back from the same source AST `let` to compute V_{k+1}'s
    // ctor args. Without an annotation the post-desugar type
    // checker may pick a different shape (e.g. `i64[]` vs the
    // `i64[3]` we baked into the variant field), causing a layout
    // mismatch when the value flows through the variant cell.
    let body_lets = collect_let_types(&f.params, &f.body, fn_returns, classes).map_err(|missing| {
        AsyncLowerError {
            fn_name: f.name,
            span: f.span,
            reason: format!(
                "async fn `{}`: `let {} = ...` — the desugar couldn't \
                 infer this binding's type from the RHS shape. Add an \
                 explicit `let {}: T = ...` annotation (the AST-stage \
                 desugar covers a small subset of RHS shapes; full type \
                 inference runs after the desugar).",
                f.name.as_str(),
                missing.as_str(),
                missing.as_str(),
            ),
        }
    })?;
    let body_lets_map: HashMap<Symbol, Type> = body_lets.iter().cloned().collect();
    stamp_inferred_let_types_block(&mut f.body, &body_lets_map);
    match state_machine::lower(&f, &body_lets, enclosing_class, enums) {
        state_machine::LowerOutput::Built(out) => Ok(AsyncLowerOutput::StateMachine {
            wrapper: out.wrapper,
            state_class: out.state_ref_class,
            poll_fn: out.poll_fn,
            state_enum: Some(out.state_enum),
        }),
        state_machine::LowerOutput::NoAwait => {
            // Defensive: body_contains_await above should have caught this.
            unreachable!("v2 returned NoAwait after body_contains_await=true")
        }
        state_machine::LowerOutput::NeedsFallback => {
            // `NeedsFallback` now means the body shape isn't covered
            // by the segment builder. (Generic async fns are no
            // longer rejected up-front.)
            let reason = format!(
                "async fn `{}`: this body shape isn't covered by the \
                 state-machine lowering yet. Refactor to a supported \
                 shape (sequential `let v = await ...`, \
                 `if/elif/else`, `while` with await-free cond, \
                 `match`, `for v in s..e`, `loop`).",
                f.name.as_str(),
            );
            Err(AsyncLowerError {
                fn_name: f.name,
                span: f.span,
                reason,
            })
        }
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
