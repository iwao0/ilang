//! Mini type-inferencer for un-annotated `let` bindings inside an
//! `async fn` body.
//!
//! The state enum's per-variant fields are typed from these recovered
//! types, so any binding the segment builder later refers to needs a
//! known type even when the user omitted the annotation. Without
//! this, the post-desugar type checker may pick a different shape
//! (notably `i64[]` vs `i64[3]` for fixed-size array literals),
//! causing a layout mismatch at the variant cell.

use std::collections::{HashMap, HashSet};

use ilang_ast::{Block, ClassDecl, Expr, ExprKind, Param, Span, Stmt, StmtKind, Symbol, Type};

/// Side tables threaded through the mini-inferencer. `fn_returns`
/// covers top-level fn return types; `classes` is consulted when
/// the RHS is `obj.field` or `obj.method(...)` and we need to find
/// the receiver class's field type / method return type.
struct InferCtx<'a> {
    fn_returns: &'a HashMap<Symbol, Type>,
    classes: &'a HashMap<Symbol, ClassDecl>,
}

/// Walk the body and stamp inferred types onto un-annotated `let`s.
/// Done in-place; only writes when the let had `ty=None`.
pub(super) fn stamp_inferred_let_types_block(b: &mut Block, m: &HashMap<Symbol, Type>) {
    for s in b.stmts.iter_mut() {
        stamp_inferred_let_types_stmt(s, m);
    }
    if let Some(t) = b.tail.as_deref_mut() {
        stamp_inferred_let_types_expr(t, m);
    }
}

fn stamp_inferred_let_types_stmt(s: &mut Stmt, m: &HashMap<Symbol, Type>) {
    match &mut s.kind {
        StmtKind::Let { name, ty, value, .. } => {
            if ty.is_none() {
                if let Some(t) = m.get(name) {
                    *ty = Some(t.clone());
                }
            }
            stamp_inferred_let_types_expr(value, m);
        }
        StmtKind::LetTuple { value, .. } | StmtKind::LetStruct { value, .. } => {
            stamp_inferred_let_types_expr(value, m);
        }
        StmtKind::Expr(e) => stamp_inferred_let_types_expr(e, m),
    }
}

fn stamp_inferred_let_types_expr(e: &mut Expr, m: &HashMap<Symbol, Type>) {
    match &mut e.kind {
        ExprKind::Block(b) => stamp_inferred_let_types_block(b, m),
        ExprKind::If { then_branch, else_branch, .. } => {
            stamp_inferred_let_types_block(then_branch, m);
            if let Some(eb) = else_branch.as_deref_mut() {
                stamp_inferred_let_types_expr(eb, m);
            }
        }
        ExprKind::While { body, .. } | ExprKind::Loop { body } => {
            stamp_inferred_let_types_block(body, m);
        }
        ExprKind::ForIn { body, .. } => stamp_inferred_let_types_block(body, m),
        ExprKind::Match { arms, .. } => {
            for a in arms.iter_mut() {
                stamp_inferred_let_types_expr(&mut a.body, m);
            }
        }
        _ => {}
    }
}

/// Collect inferred types for every `let` binding visible to the
/// state machine. Returns `Err(name)` when the binding is un-annotated
/// AND the inferencer can't handle its RHS shape.
pub(super) fn collect_let_types(
    params: &[Param],
    b: &Block,
    fn_returns: &HashMap<Symbol, Type>,
    classes: &HashMap<Symbol, ClassDecl>,
) -> Result<Vec<(Symbol, Type)>, Symbol> {
    let ctx = InferCtx { fn_returns, classes };
    let mut env: HashMap<Symbol, Type> = HashMap::new();
    for p in params {
        env.insert(p.name, p.ty.clone());
    }
    let mut out: Vec<(Symbol, Type)> = Vec::new();
    let mut seen: HashSet<Symbol> = HashSet::new();
    walk_block_for_lets(b, &mut env, &mut out, &mut seen, &ctx)?;
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
    ctx: &InferCtx<'_>,
) -> Result<(), Symbol> {
    for s in &b.stmts {
        if let StmtKind::Let { name, ty, value, .. } = &s.kind {
            if seen.insert(*name) {
                let t = if let Some(t) = ty {
                    t.clone()
                } else {
                    match infer_let_rhs(value, env, ctx) {
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
            walk_expr_for_lets(value, env, out, seen, ctx)?;
        } else if let StmtKind::Expr(e) = &s.kind {
            // Recurse into `while` bodies so loop-local lets get
            // a state-class field (any binding live across the
            // back-edge needs storage).
            if let ExprKind::While { body, .. } = &e.kind {
                walk_block_for_lets(body, env, out, seen, ctx)?;
            }
        }
    }
    if let Some(tail) = &b.tail {
        walk_if_tail_for_lets(tail, env, out, seen, ctx)?;
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
    ctx: &InferCtx<'_>,
) -> Result<(), Symbol> {
    match &e.kind {
        ExprKind::Block(b) => walk_block_for_lets(b, env, out, seen, ctx)?,
        ExprKind::If { then_branch, else_branch, .. } => {
            walk_block_for_lets(then_branch, env, out, seen, ctx)?;
            if let Some(eb) = else_branch {
                walk_expr_for_lets(eb, env, out, seen, ctx)?;
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
                walk_expr_for_lets(&arm.body, env, out, seen, ctx)?;
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
    ctx: &InferCtx<'_>,
) -> Result<(), Symbol> {
    match &e.kind {
        ExprKind::If { then_branch, else_branch, .. } => {
            walk_block_for_lets(then_branch, env, out, seen, ctx)?;
            if let Some(eb) = else_branch {
                walk_if_tail_for_lets(eb, env, out, seen, ctx)?;
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
                walk_if_tail_for_lets(&arm.body, env, out, seen, ctx)?;
            }
        }
        ExprKind::Block(b) => {
            walk_block_for_lets(b, env, out, seen, ctx)?;
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
    ctx: &InferCtx<'_>,
) -> Option<Type> {
    match &e.kind {
        ExprKind::Int(_) => Some(Type::I64),
        ExprKind::Float(_) => Some(Type::F64),
        ExprKind::Bool(_) => Some(Type::Bool),
        ExprKind::Str(_) => Some(Type::Str),
        ExprKind::Array(es) => {
            // Element type comes from the first element. Empty
            // arrays still need a `let xs: T[] = []` annotation
            // (no element to infer from).
            let first = es.first()?;
            let elem = infer_let_rhs(first, env, ctx)?;
            Some(Type::Array { elem: Box::new(elem), fixed: Some(es.len()) })
        }
        ExprKind::Var(n) => env.get(n).cloned(),
        ExprKind::Call { callee, .. } => ctx.fn_returns.get(callee).cloned(),
        ExprKind::New { class, .. } => Some(Type::Object(*class)),
        ExprKind::Await(inner) => {
            let t = infer_let_rhs(inner, env, ctx)?;
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
                            let inner = infer_let_rhs(&args[0], env, ctx)?;
                            return Some(Type::generic("Promise", vec![inner]));
                        }
                        "reject" => {
                            return Some(Type::generic("Promise", vec![Type::Unit]));
                        }
                        "$promise.pending" => {
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
            // Instance method call: infer the receiver's class and
            // look up the method's declared return type.
            let ot = infer_let_rhs(obj, env, ctx)?;
            let class_name = match &ot {
                Type::Object(n) => *n,
                Type::Generic(g) => g.base,
                _ => return None,
            };
            let cd = ctx.classes.get(&class_name)?;
            // Both regular methods and static methods can be reached
            // via `obj.method(...)` syntax; check both lists.
            let m = cd
                .methods
                .iter()
                .chain(cd.static_methods.iter())
                .find(|m| m.name == *method)?;
            let ret = m.ret.clone().unwrap_or(Type::Unit);
            // async methods see Promise-wrapped returns from callers.
            if m.is_async {
                let already_promise = matches!(
                    &ret,
                    Type::Generic(g) if g.base.as_str() == "Promise"
                );
                if already_promise {
                    Some(ret)
                } else {
                    Some(Type::generic("Promise", vec![ret]))
                }
            } else {
                Some(ret)
            }
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let lt = infer_let_rhs(lhs, env, ctx)?;
            let rt = infer_let_rhs(rhs, env, ctx)?;
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
        ExprKind::Unary { expr, .. } => infer_let_rhs(expr, env, ctx),
        ExprKind::Index { obj, .. } => {
            // `arr[i]` — element type of an Array. Other indexable
            // types (Map etc.) aren't covered here; an explicit
            // annotation lets the user override.
            let ot = infer_let_rhs(obj, env, ctx)?;
            match ot {
                Type::Array { elem, .. } => Some(*elem),
                _ => None,
            }
        }
        ExprKind::Field { obj, name } => {
            // `obj.field` — look up the field type on the receiver's
            // class. Only works when the receiver's type resolves to
            // a known user class.
            let ot = infer_let_rhs(obj, env, ctx)?;
            let class_name = match &ot {
                Type::Object(n) => *n,
                Type::Generic(g) => g.base,
                _ => return None,
            };
            let cd = ctx.classes.get(&class_name)?;
            cd.fields
                .iter()
                .find(|f| f.name == *name)
                .map(|f| f.ty.clone())
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
            if let Some(ty) = infer_let_rhs(&then_synth, env, ctx) {
                return Some(ty);
            }
            if let Some(eb) = else_branch {
                return infer_let_rhs(eb, env, ctx);
            }
            None
        }
        ExprKind::Match { arms, .. } => {
            // Type of match = type of any arm body. Try the first.
            for a in arms.iter() {
                if let Some(ty) = infer_let_rhs(&a.body, env, ctx) {
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
                        infer_let_rhs(value, &inner_env, ctx)
                    });
                    if let Some(t) = t {
                        inner_env.insert(*name, t);
                    }
                }
            }
            b.tail
                .as_deref()
                .and_then(|t| infer_let_rhs(t, &inner_env, ctx))
        }
        _ => None,
    }
}
