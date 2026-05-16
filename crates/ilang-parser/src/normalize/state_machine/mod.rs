//! `async fn` → enum-variant state machine.
//!
//! For an async fn with N `await` sites, the body splits into
//! N+1 **segments** (straight-line chunks separated by awaits and
//! other control-flow boundaries). The state lives as a heap-
//! allocated enum value with one variant per segment; each variant
//! carries exactly the locals live at that segment's entry. A
//! small wrapper class (`__<name>_StateRef`) holds the enum cell
//! plus the result Promise so the `.then` callbacks can mutate
//! state across the chain.
//!
//! ## Sub-modules
//!
//! - `pre_desugar`: rewrites `loop` / `while await cond` / `for-in`
//!   into the equivalent `while` shape so the segment builder
//!   only has to handle one loop form.
//! - `pattern`: type/shape helpers for if-else / match — coercing
//!   `else if` chains into Blocks, recognising mid-body
//!   `let X = if/match { ...await... }` join points, and
//!   resolving precise types for pattern-introduced bindings
//!   (`some(v)`, `ok(v)`, `Box.hold(s)`, …).
//! - `segments`: the segment graph itself. `Segment` /
//!   `SegTerm` / `MatchTArm` data types, body-shape checks
//!   (`body_is_supported`), and `build_segments` which walks the
//!   body and produces the segment list.
//! - `gen_items`: AST generators — given the segment list, emit
//!   the state `enum`, the `__<name>_StateRef` class, the
//!   `__<name>_poll` driver fn (a `loop { match … }`), and the
//!   original-named wrapper fn that allocates state and kicks the
//!   first poll.
//!
//! `lower` (this file) is the orchestrator: shape check, then
//! call `build_segments` and the four `gen_*` helpers, then
//! package the four items in a `StateMachineOutput`.
//!
//! ## Generated layout (sketch)
//!
//! For
//!
//! ```ignore
//! async fn run(p: Promise<i64>): i64 {
//!     let c = new Counter(10)
//!     let v = await p
//!     c.n + v
//! }
//! ```
//!
//! we emit roughly
//!
//! ```ignore
//! enum __run_State {
//!     S0 { p: Promise<i64> }
//!     S1 { p: Promise<i64>, c: Counter, v: i64 }
//! }
//! class __run_StateRef { current: __run_State, __async_promise: Promise<i64> ... }
//! fn __run_poll(state_ref, _) {
//!     loop {
//!         match state_ref.current {
//!             S0 { p } { let c = new Counter(10);
//!                        let _ = p.then(fn(v) { state_ref.current = S1{p,c,v}; __run_poll(...); v });
//!                        return }
//!             S1 { p, c, v } { Promise.__settleResolve(state_ref.__async_promise, c.n + v); return }
//!         }
//!     }
//! }
//! fn run(p) -> Promise<i64> { ... allocate StateRef, call __run_poll, return prom ... }
//! ```

use std::collections::HashMap;

use ilang_ast::{
    Block, ClassDecl, CtorArgs, EnumDecl, Expr, ExprKind, FnDecl, Span, Stmt, StmtKind, Symbol,
    Type,
};

mod gen_items;
mod pattern;
mod pre_desugar;
mod segments;
pub use pre_desugar::{desugar_for_in_with_await, desugar_loop_to_while, desugar_while_cond_await};
use gen_items::{gen_poll_fn, gen_state_enum, gen_state_ref_class, gen_wrapper_fn};
use segments::build_segments;

/// AST output of `lower` for an await-containing async fn body.
pub struct StateMachineOutput {
    pub wrapper: FnDecl,
    pub state_enum: EnumDecl,
    pub state_ref_class: ClassDecl,
    pub poll_fn: FnDecl,
}

/// The three outcomes of `lower`.
pub enum LowerOutput {
    /// Body had no awaits — caller falls back to the trivial
    /// `Promise.resolve(...)` wrap (not handled here).
    NoAwait,
    /// Body shape isn't covered by the segment builder. Caller
    /// surfaces an error to the user.
    NeedsFallback,
    /// Built the enum-variant state machine.
    Built(StateMachineOutput),
}


// --- Shared await-detection helpers ----------------------------------
//
// Used by both the `pre_desugar` pass (to decide whether a loop /
// for-in body needs rewriting) and the `segments` builder (body
// shape detection).

pub(super) fn block_has_await(b: &Block) -> bool {
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

pub(super) fn expr_has_await(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Await(_) => true,
        ExprKind::If { cond, then_branch, else_branch } => {
            expr_has_await(cond)
                || block_has_await(then_branch)
                || else_branch.as_deref().is_some_and(expr_has_await)
        }
        ExprKind::While { cond, body } => expr_has_await(cond) || block_has_await(body),
        ExprKind::Match { scrutinee, arms } => {
            expr_has_await(scrutinee) || arms.iter().any(|a| expr_has_await(&a.body))
        }
        ExprKind::Block(b) => block_has_await(b),
        _ => false,
    }
}

// --- AST construction helpers ---------------------------------------

pub(super) fn mk_var(name: Symbol, span: Span) -> Expr {
    Expr::new(ExprKind::Var(name), span)
}
pub(super) fn mk_int(n: i64, span: Span) -> Expr {
    Expr::new(ExprKind::Int(n), span)
}
pub(super) fn mk_field(obj: Expr, name: Symbol, span: Span) -> Expr {
    Expr::new(ExprKind::Field { obj: Box::new(obj), name }, span)
}
pub(super) fn mk_assign_field(obj: Expr, field: Symbol, value: Expr, span: Span) -> Expr {
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
pub(super) fn mk_method_call(obj: Expr, method: Symbol, args: Vec<Expr>, span: Span) -> Expr {
    Expr::new(
        ExprKind::MethodCall {
            obj: Box::new(obj),
            method,
            args: args.into_boxed_slice(),
        },
        span,
    )
}
pub(super) fn mk_call(callee: Symbol, args: Vec<Expr>, span: Span) -> Expr {
    Expr::new(
        ExprKind::Call { callee, args: args.into_boxed_slice() },
        span,
    )
}
pub(super) fn mk_let(name: Symbol, ty: Option<Type>, value: Expr, span: Span) -> Stmt {
    Stmt::new(
        StmtKind::Let { is_pub: false, is_const: false, name, ty, value },
        span,
    )
}
pub(super) fn mk_expr_stmt(e: Expr, span: Span) -> Stmt {
    Stmt::new(StmtKind::Expr(e), span)
}
pub(super) fn mk_enum_ctor_struct(
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
    // Generic `async fn first<T>(...)` would need the synthesized
    // state enum / class / poll fn to also carry the user fn's
    // type params, but ilang's typecheck currently rejects
    // `new GenericClass<T>(...)` inside a generic fn body
    // ("expected T, got T"), even for hand-written code. Until
    // that's fixed, surface a clearer error here than the
    // downstream "undefined class T" the MIR lower would emit.
    if !f.type_params.is_empty() {
        return LowerOutput::NeedsFallback;
    }
    if !segments::body_is_supported(&f.body) {
        return LowerOutput::NeedsFallback;
    }
    let span = f.span;
    // If the user wrote `async fn foo(): Promise<U>` explicitly,
    // treat the inner result type as `U` (not `Promise<U>`) so we
    // don't end up generating a `Promise<Promise<U>>` wrapper. The
    // outer `Promise<U>` already matches the user's declared shape.
    let declared_ret = f.ret.clone().unwrap_or(Type::Unit);
    let (inner_ret, promise_ret) = match &declared_ret {
        Type::Generic(g) if g.base.as_str() == "Promise" && g.args.len() == 1 => {
            (g.args[0].clone(), declared_ret.clone())
        }
        _ => (
            declared_ret.clone(),
            Type::generic("Promise", vec![declared_ret.clone()]),
        ),
    };

    let _ = inner_ret;
    let segments =
        build_segments(&f.body, &f.params, body_lets, enclosing_class, span, enums);
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
