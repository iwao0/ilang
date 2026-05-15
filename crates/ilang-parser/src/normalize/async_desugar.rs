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
    Block, ClassDecl, EnumDecl, Expr, ExprKind, FnDecl, Item, Param,
    Program, Span, Stmt, StmtKind, Symbol, Type,
};

use super::state_machine_v2;


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
    }
    let mut errors: Vec<AsyncLowerError> = Vec::new();
    let mut items: Vec<Item> = Vec::with_capacity(prog.items.len());
    for item in prog.items {
        match item {
            Item::Fn(f) => match lower_async_fn(f, &fn_returns, None, &enums) {
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
                let lowered = lower_class(c, &mut items, &mut errors, &fn_returns, &enums);
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
) -> ClassDecl {
    let class_name = c.name;
    let methods: Vec<FnDecl> = std::mem::take(&mut c.methods)
        .into_iter()
        .map(|m| match lower_async_fn(m, fn_returns, Some(class_name), enums) {
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
) -> Result<AsyncLowerOutput, AsyncLowerError> {
    if !f.is_async {
        return Ok(AsyncLowerOutput::Single(f));
    }
    // Pre-pass: lift every `await E` that appears inside a sub-
    // expression into its own `let __await_tN = await E` statement
    // above the use site. The state-machine builder only handles
    // the canonical "await as direct let RHS" form; this pass
    // normalizes shapes like `foo(await p, await q)` and
    // `bar(await p) + 1` into that form.
    // Pre-passes (run before lift_subexpr_awaits so that any awaits
    // introduced by the rewrites flow through the canonicaliser):
    //   0. `loop { ...await... }` → `while true { ... }`.
    f.body = state_machine_v2::desugar_loop_to_while(f.body);
    //   1. `while await cond { ... }` → `while true { let cv = await cond; if !cv { break } ... }`
    f.body = state_machine_v2::desugar_while_cond_await(f.body);
    f.body = lift_subexpr_awaits(f.body);
    //   2. `for v in s..e { ...await... }` → equivalent while loop.
    //      (Runs AFTER lift since for-in's iter rarely needs lifting.)
    f.body = state_machine_v2::desugar_for_in_with_await(f.body);

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
        }));
    }

    // ≥1 await: lower to enum-variant state machine via v2.
    let body_lets = collect_let_types(&f.params, &f.body, fn_returns).map_err(|missing| {
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
    match state_machine_v2::lower(&f, &body_lets, enclosing_class, enums) {
        state_machine_v2::LowerOutput::Built(out) => Ok(AsyncLowerOutput::StateMachine {
            wrapper: out.wrapper,
            state_class: out.state_ref_class,
            poll_fn: out.poll_fn,
            state_enum: Some(out.state_enum),
        }),
        state_machine_v2::LowerOutput::NoAwait => {
            // Defensive: body_contains_await above should have caught this.
            unreachable!("v2 returned NoAwait after body_contains_await=true")
        }
        state_machine_v2::LowerOutput::NeedsFallback => Err(AsyncLowerError {
            fn_name: f.name,
            span: f.span,
            reason: format!(
                "async fn `{}`: this body shape isn't covered by the \
                 state-machine lowering yet (e.g. `for-in` / `loop` with \
                 awaits, or await inside a `while` cond). Refactor to a \
                 supported shape (sequential `let v = await ...`, \
                 `if/elif/else`, `while` with await-free cond, `match`).",
                f.name.as_str(),
            ),
        }),
    }
}

/// Walk a block tree to determine whether any `await` occurs anywhere
/// inside. Used to decide between the trivial `Promise.resolve(...)`
/// wrap and the state-machine lowering.
fn body_contains_await(b: &Block) -> bool {
    for s in &b.stmts {
        if stmt_contains_await(s) {
            return true;
        }
    }
    if let Some(t) = b.tail.as_deref() {
        if expr_contains_await(t) {
            return true;
        }
    }
    false
}

fn stmt_contains_await(s: &Stmt) -> bool {
    match &s.kind {
        StmtKind::Let { value, .. }
        | StmtKind::LetTuple { value, .. }
        | StmtKind::LetStruct { value, .. } => expr_contains_await(value),
        StmtKind::Expr(e) => expr_contains_await(e),
    }
}

fn expr_contains_await(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Await(_) => true,
        ExprKind::Block(b) => body_contains_await(b),
        ExprKind::If { cond, then_branch, else_branch } => {
            expr_contains_await(cond)
                || body_contains_await(then_branch)
                || else_branch.as_deref().is_some_and(expr_contains_await)
        }
        ExprKind::IfLet { expr, then_branch, else_branch, .. } => {
            expr_contains_await(expr)
                || body_contains_await(then_branch)
                || else_branch.as_deref().is_some_and(expr_contains_await)
        }
        ExprKind::While { cond, body } => expr_contains_await(cond) || body_contains_await(body),
        ExprKind::Loop { body } => body_contains_await(body),
        ExprKind::ForIn { iter, body, .. } => expr_contains_await(iter) || body_contains_await(body),
        ExprKind::Match { scrutinee, arms } => {
            expr_contains_await(scrutinee)
                || arms.iter().any(|a| expr_contains_await(&a.body))
        }
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
            expr_contains_await(lhs) || expr_contains_await(rhs)
        }
        ExprKind::Unary { expr, .. }
        | ExprKind::Cast { expr, .. }
        | ExprKind::TypeTest { expr, .. }
        | ExprKind::TypeDowncast { expr, .. } => expr_contains_await(expr),
        ExprKind::Some(e) => expr_contains_await(e),
        ExprKind::Return(opt) | ExprKind::Break(opt) => {
            opt.as_deref().is_some_and(expr_contains_await)
        }
        ExprKind::Assign { value, .. } => expr_contains_await(value),
        ExprKind::AssignField { obj, value, .. } => {
            expr_contains_await(obj) || expr_contains_await(value)
        }
        ExprKind::AssignIndex { obj, index, value } => {
            expr_contains_await(obj) || expr_contains_await(index) || expr_contains_await(value)
        }
        ExprKind::Call { args, .. }
        | ExprKind::SuperCall { args, .. }
        | ExprKind::New { args, .. } => args.iter().any(expr_contains_await),
        ExprKind::MethodCall { obj, args, .. } => {
            expr_contains_await(obj) || args.iter().any(expr_contains_await)
        }
        ExprKind::Field { obj, .. } => expr_contains_await(obj),
        ExprKind::Index { obj, index } => expr_contains_await(obj) || expr_contains_await(index),
        ExprKind::Tuple(es) | ExprKind::Array(es) => es.iter().any(expr_contains_await),
        ExprKind::Range { start, end, .. } => {
            start.as_deref().is_some_and(expr_contains_await)
                || end.as_deref().is_some_and(expr_contains_await)
        }
        ExprKind::FnExpr { .. } | ExprKind::Closure { .. } => false,
        _ => false,
    }
}

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
            if seen.insert(*name) {
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
            // Also walk the let's RHS — for `let r = if-else { ... }` /
            // `let r = match { ... }`, inner arm-local lets need
            // state-class fields too.
            walk_expr_for_lets(value, env, out, seen, fn_returns)?;
        } else if let StmtKind::Expr(e) = &s.kind {
            // Recurse into `while` bodies so loop-local lets get
            // a state-class field (any binding live across the
            // back-edge needs storage).
            if let ExprKind::While { body, .. } = &e.kind {
                walk_block_for_lets(body, env, out, seen, fn_returns)?;
            }
        }
    }
    if let Some(tail) = &b.tail {
        walk_if_tail_for_lets(tail, env, out, seen, fn_returns)?;
    }
    Ok(())
}

/// Walk a (sub-)expression looking for lets that need a state-
/// class field. Used for `let r = if-else { let inner = ... }` style
/// mid-body RHS expressions where inner bindings persist across
/// await boundaries.
fn walk_expr_for_lets(
    e: &Expr,
    env: &mut HashMap<Symbol, Type>,
    out: &mut Vec<(Symbol, Type)>,
    seen: &mut HashSet<Symbol>,
    fn_returns: &HashMap<Symbol, Type>,
) -> Result<(), Symbol> {
    match &e.kind {
        ExprKind::Block(b) => walk_block_for_lets(b, env, out, seen, fn_returns)?,
        ExprKind::If { then_branch, else_branch, .. } => {
            walk_block_for_lets(then_branch, env, out, seen, fn_returns)?;
            if let Some(eb) = else_branch {
                walk_expr_for_lets(eb, env, out, seen, fn_returns)?;
            }
        }
        ExprKind::Match { arms, .. } => {
            for arm in arms.iter() {
                for binding in pattern_binding_names(&arm.pattern) {
                    if seen.insert(binding) {
                        let t = Type::I64;
                        env.insert(binding, t.clone());
                        out.push((binding, t));
                    }
                }
                walk_expr_for_lets(&arm.body, env, out, seen, fn_returns)?;
            }
        }
        _ => {}
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
        ExprKind::Match { scrutinee: _, arms } => {
            // Pattern bindings get rewritten to `state.<name>` field
            // accesses by the variable rewriter — they need a slot
            // in the state class. The binding's type isn't recoverable
            // at the desugar stage without typecheck info (each
            // enum variant's payload signature would have to be
            // looked up). We register the binding with `Type::I64`
            // as a placeholder: it matches the runtime ABI of every
            // pointer / numeric, and the type checker accepts the
            // post-desugar field assignment as long as the actual
            // bound value's MIR type lines up. Patterns that need
            // type-precise field declarations (e.g. heap strings
            // that the cascade must release) are a follow-up.
            for arm in arms.iter() {
                for binding_name in pattern_binding_names(&arm.pattern) {
                    if seen.insert(binding_name) {
                        let t = Type::I64;
                        env.insert(binding_name, t.clone());
                        out.push((binding_name, t));
                    }
                }
                // Then walk the arm body for further lets.
                walk_if_tail_for_lets(&arm.body, env, out, seen, fn_returns)?;
            }
        }
        ExprKind::Block(b) => {
            walk_block_for_lets(b, env, out, seen, fn_returns)?;
        }
        _ => {}
    }
    Ok(())
}

/// Collect every binding name introduced by a pattern (variant
/// payload tuples / structs). Wildcards and primitive literal
/// patterns produce nothing.
fn pattern_binding_names(p: &ilang_ast::Pattern) -> Vec<Symbol> {
    match &p.kind {
        ilang_ast::PatternKind::Variant { bindings, .. } => match bindings {
            ilang_ast::PatternBindings::Unit => Vec::new(),
            ilang_ast::PatternBindings::Tuple(names) => names
                .iter()
                .filter(|n| n.as_str() != "_")
                .copied()
                .collect(),
            ilang_ast::PatternBindings::Struct(pairs) => pairs
                .iter()
                .map(|(_, bind)| *bind)
                .filter(|n| n.as_str() != "_")
                .collect(),
        },
        _ => Vec::new(),
    }
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
        ExprKind::New { class, .. } => Some(Type::Object(*class)),
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
        ExprKind::Index { obj, .. } => {
            // `arr[i]` — element type of an Array. Other indexable
            // types (Map etc.) aren't covered here; an explicit
            // annotation lets the user override.
            let ot = infer_let_rhs(obj, env, fn_returns)?;
            match ot {
                Type::Array { elem, .. } => Some(*elem),
                _ => None,
            }
        }
        ExprKind::If { then_branch, else_branch, .. } => {
            // Type of `if-else` = type of either arm. Walk the
            // then-branch as a synthetic block (which threads inner
            // let bindings into the env) before checking its tail;
            // fall back to else.
            let then_synth = Expr::new(
                ExprKind::Block(then_branch.clone()),
                Span::dummy(),
            );
            if let Some(ty) = infer_let_rhs(&then_synth, env, fn_returns) {
                return Some(ty);
            }
            if let Some(eb) = else_branch {
                return infer_let_rhs(eb, env, fn_returns);
            }
            None
        }
        ExprKind::Match { arms, .. } => {
            // Type of match = type of any arm body. Try the first.
            for a in arms.iter() {
                if let Some(ty) = infer_let_rhs(&a.body, env, fn_returns) {
                    return Some(ty);
                }
            }
            None
        }
        ExprKind::Block(b) => {
            // Type of a block = its tail's type. Track inner let
            // bindings as we walk so the tail can reference them.
            let mut inner_env = env.clone();
            for s in &b.stmts {
                if let StmtKind::Let { name, ty, value, .. } = &s.kind {
                    let t = ty.clone().or_else(|| {
                        infer_let_rhs(value, &inner_env, fn_returns)
                    });
                    if let Some(t) = t {
                        inner_env.insert(*name, t);
                    }
                }
            }
            b.tail
                .as_deref()
                .and_then(|t| infer_let_rhs(t, &inner_env, fn_returns))
        }
        _ => None,
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
        // `if` cond and `match` scrutinee are evaluated
        // unconditionally before any arm runs, so we CAN lift
        // awaits there. The arm bodies themselves are NOT
        // descended into — those are scope-specific and the
        // state-machine handles awaits inside them via separate
        // dispatch. Same logic doesn't apply to `while` cond
        // (re-evaluated each iter) — leave those alone.
        ExprKind::If { cond, then_branch, else_branch } => {
            let cond = Box::new(lift_in_expr(*cond, lifts, counter, false));
            Expr::new(
                ExprKind::If { cond, then_branch, else_branch },
                span,
            )
        }
        ExprKind::Match { scrutinee, arms } => {
            let scrutinee = Box::new(lift_in_expr(*scrutinee, lifts, counter, false));
            Expr::new(ExprKind::Match { scrutinee, arms }, span)
        }
        // Everything else that introduces a new scope (blocks, while,
        // loop, closures) is NOT descended into. Awaits inside are
        // either rejected by the analyser or handled by the
        // state-machine builder.
        kind => Expr::new(kind, span),
    }
}

