//! Extracted from `checker/mod.rs`. The dispatch (`check_expr` /
//! `check_expr_inner`) lives here together with the small
//! self-contained variants; the largest variant arms live in the
//! sibling submodules below.

#![allow(unused_imports)]

mod access;
mod calls;
mod casts;
mod match_ctrl;

use std::collections::{HashMap, HashSet};

use ilang_ast::{
    Block, ClassDecl, CtorArgs, EnumDecl, Expr, ExprKind, FieldDecl, FnDecl, Item, Param,
    PatternBindings, PatternKind, Program, Span, Stmt, StmtKind, Symbol, Type, UnOp,
    VariantPayload,
};

use crate::error::TypeError;
use crate::ops::{assignable, bin_result, int_literal_fits};

use super::*;

/// `&path` accepts a local variable (or `this`) optionally
/// followed by a chain of field accesses (`x`, `x.f`, `x.f.g`,
/// `this.f`, `this.f.g`, ...). Any other shape (indexing, calls,
/// parenthesised expressions, etc.) is rejected at this level —
/// the MIR lowerer relies on the AST matching one of these forms.
fn is_addr_path(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Var(_) | ExprKind::This => true,
        ExprKind::Field { obj, .. } => is_addr_path(obj),
        _ => false,
    }
}

/// `true` when `e` unconditionally transfers control out of the
/// enclosing expression — `return` / `break` / `continue`, or a
/// `Block` whose tail (or some unconditional statement) does so.
/// Used by match-arm and (later) if/else type-checking to skip
/// arms that don't actually produce a value for the join.
pub(super) fn arm_body_diverges(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Return(_) | ExprKind::Break(_) | ExprKind::Continue => true,
        ExprKind::Block(b) => {
            for s in &b.stmts {
                if let StmtKind::Expr(inner) = &s.kind {
                    if arm_body_diverges(inner) {
                        return true;
                    }
                }
            }
            b.tail.as_ref().map(|t| arm_body_diverges(t)).unwrap_or(false)
        }
        _ => false,
    }
}

impl TypeChecker {
    pub(super) fn check_expr(
        &self,
        expr: &Expr,
        env: &Vars,
        ret_ty: Option<&Type>,
        in_class: Option<Symbol>,
        loop_depth: u32,
    ) -> Result<Type, TypeError> {
        let t = self.check_expr_inner(expr, env, ret_ty, in_class, loop_depth)?;
        // Outside `@extern(C) {}`, no expression may evaluate to a
        // raw C pointer / `char` / `void` / `size_t` / `ssize_t`.
        // This rejects helper calls like `cstrFromString(...)` and
        // FFI fn calls returning C-only types from user code, forcing
        // the user to wrap them in an `@extern(C)` fn that yields a
        // regular ilang type.
        if !*self.in_extern_c.borrow() {
            if let Some(c_only) = first_c_only_type(&t) {
                return Err(TypeError::Unsupported {
                    what: format!(
                        "expression of type {t} is only allowed inside an \
                         @extern(C) {{ ... }} block (contains the C-only type \
                         {c_only})"
                    ),
                    span: expr.span,
                });
            }
        }
        Ok(t)
    }

    pub(super) fn check_expr_inner(
        &self,
        expr: &Expr,
        env: &Vars,
        ret_ty: Option<&Type>,
        in_class: Option<Symbol>,
        loop_depth: u32,
    ) -> Result<Type, TypeError> {
        let span = expr.span;
        match &expr.kind {
            ExprKind::StructLit { class, fields, .. } => {
                // Look up the class signature. A literal against an
                // unknown class name is the same error as `new
                // BogusName()`.
                let cls = self.classes.get(class).ok_or_else(|| {
                    TypeError::UndefinedClass {
                        name: class.clone(),
                        span,
                    }
                })?;
                // Reject duplicates first — `Foo { x: 1, x: 2 }` is
                // ambiguous regardless of what `Foo` is.
                let mut seen: HashSet<Symbol> = HashSet::with_capacity(fields.len());
                for (fname, _) in fields.iter() {
                    if !seen.insert(fname.clone()) {
                        return Err(TypeError::Unsupported {
                            what: format!(
                                "duplicate field {fname:?} in struct literal for {class:?}"
                            ),
                            span,
                        });
                    }
                }
                // Struct-literal construction is reserved for value
                // types — top-level `struct` / `union` (CRepr). ARC
                // classes must go through `new Name(...)` so their
                // `init` actually runs and required-assignment
                // invariants are enforced; allowing a literal there
                // would silently skip the constructor and let
                // partial states leak out.
                if !cls.is_repr_c {
                    return Err(TypeError::Unsupported {
                        what: format!(
                            "struct-literal construction `{class:?} {{ ... }}` is only \
                             allowed for top-level `struct` / `union` (value types); use \
                             `new {class:?}(...)` to construct a class instance"
                        ),
                        span,
                    });
                }
                // CRepr union: exactly one field is initialized
                // (variants share one storage slot — initializing
                // zero or multiple has no meaningful semantics).
                // CRepr struct: every declared field must be
                // explicitly initialized — no `init` exists to fill
                // missing slots, so a partial literal would leave
                // them at their zero-initialized default.
                if cls.is_union {
                    if fields.len() != 1 {
                        return Err(TypeError::Unsupported {
                            what: format!(
                                "union literal for {class:?} must initialize exactly \
                                 one field (got {})",
                                fields.len()
                            ),
                            span,
                        });
                    }
                } else {
                    for declared in cls.fields.keys() {
                        if !seen.contains(declared) {
                            return Err(TypeError::Unsupported {
                                what: format!(
                                    "struct literal for {class:?} is missing field \
                                     {declared:?} — CRepr struct literals must initialize \
                                     every field"
                                ),
                                span,
                            });
                        }
                    }
                }
                // Type-check each field expression against its
                // declared type. Reject unknown field names — the
                // declaration is authoritative.
                for (fname, fexpr) in fields.iter() {
                    let field_ty = cls.fields.get(fname).cloned().ok_or_else(|| {
                        TypeError::UnknownField {
                            class: class.clone(),
                            field: fname.clone(),
                            span: fexpr.span,
                        }
                    })?;
                    let vt = self.check_expr(fexpr, env, ret_ty, in_class, loop_depth)?;
                    if !self.value_assignable(fexpr, &vt, &field_ty) {
                        return Err(TypeError::Mismatch {
                            expected: field_ty,
                            got: vt,
                            span: fexpr.span,
                        });
                    }
                }
                Ok(Type::Object(class.clone()))
            }
            ExprKind::Int(_) => Ok(Type::I64),
            ExprKind::Float(_) => Ok(Type::F64),
            ExprKind::Bool(_) => Ok(Type::Bool),
            ExprKind::Str(_) => Ok(Type::Str),
            ExprKind::Closure { fn_name, .. } => {
                // The JIT runs a second typecheck on the post-hoist
                // program; by then FnExpr has been replaced with
                // Closure and the wrapper FnDecl is in self.fns.
                // The wrapper's first param is the synthetic
                // `__env: i64`; the user-facing fn type is the rest.
                let sigs = self.fns.get(fn_name).ok_or_else(|| {
                    TypeError::UndefinedVariable {
                        name: fn_name.clone(),
                        span,
                    }
                })?;
                let sig = sigs.first().expect("registered fn has sig");
                let user_params: Vec<Type> = sig.params.iter().skip(1).cloned().collect();
                Ok(Type::func(user_params, sig.ret.clone()))
            }
            ExprKind::This => match in_class {
                Some(name) => Ok(Type::Object(name.into())),
                None => Err(TypeError::ThisOutsideMethod { span }),
            },
            ExprKind::SuperCall { method, args } => {
                let class_name = in_class.ok_or_else(|| TypeError::Unsupported {
                    what: "`super` is only valid inside a class method".into(),
                    span,
                })?;
                let parent_name = self
                    .classes
                    .get(&class_name)
                    .and_then(|c| c.parent)
                    .ok_or_else(|| TypeError::Unsupported {
                        what: format!(
                            "`super` used in class {:?}, which has no parent",
                            class_name
                        ),
                        span,
                    })?;
                let parent_sig = self.classes.get(&parent_name).expect("parent registered");
                let lookup: Symbol = method.unwrap_or_else(|| "init".into());
                // Deinits chain automatically (derived first, then
                // each ancestor) — an explicit `super.deinit()` would
                // run the parent's hook a second time.
                if lookup == "deinit" {
                    return Err(TypeError::CannotCallDeinit { span });
                }
                let sigs = parent_sig.methods.get(&lookup).ok_or_else(|| {
                    TypeError::UnknownMethod {
                        class: parent_name,
                        method: lookup,
                        span,
                    }
                })?;
                if sigs.len() != 1 {
                    return Err(TypeError::Unsupported {
                        what: format!(
                            "super.{lookup}: parent's method is overloaded; \
                             super-calls require a single signature"
                        ),
                        span,
                    });
                }
                let sig = sigs[0].clone();
                self.check_args(lookup, &sig, args, env, ret_ty, in_class, loop_depth, span)?;
                Ok(sig.ret)
            }
            ExprKind::Var(n) => {
                if let Some(t) = env.get(n) {
                    return Ok(t.clone());
                }
                if let Some(class_name) = in_class {
                    if let Some(cls) = self.classes.get(&class_name) {
                        if let Some(t) = cls.fields.get(n) {
                            return Ok(t.clone());
                        }
                    }
                }
                // First-class function: a bare reference to a top-level
                // `fn` becomes a function value (`Type::Fn(...)`). For
                // overloaded names this is ambiguous — disambiguation
                // would need an explicit type annotation we don't have
                // syntax for, so reject.
                if let Some(sigs) = self.fns.get(n) {
                    if sigs.len() != 1 {
                        return Err(TypeError::Unsupported {
                            what: format!(
                                "fn {n:?} has {} overloads — bare references to overloaded \
                                 fns are ambiguous; call them directly with arguments",
                                sigs.len()
                            ),
                            span,
                        });
                    }
                    let sig = &sigs[0];
                    return Ok(Type::func(sig.params.clone(), sig.ret.clone()));
                }
                Err(TypeError::UndefinedVariable {
                    name: n.clone(),
                    span,
                })
            }
            ExprKind::Unary { op, expr: inner } => {
                // `&name` is FFI-scoped: only valid inside an
                // @extern(C) context, and only on a name that
                // resolves to a local variable. The result type is
                // `*T` where T is the local's type. The address-of
                // is materialised by the MIR lowerer (`AddrOfLocal`);
                // here we just gate-keep + assign the type.
                if matches!(op, UnOp::AddrOf) {
                    if !*self.in_extern_c.borrow() {
                        return Err(TypeError::Unsupported {
                            what: "`&` (address-of) is only allowed inside an @extern(C) block".into(),
                            span,
                        });
                    }
                    // Allowed shapes: `&local`, `&local.f1`,
                    // `&local.f1.f2....fn`. The root must be a plain
                    // local; intermediate hops are field accesses.
                    // Each intermediate must be a class (Object); the
                    // leaf may be any field type.
                    if !is_addr_path(inner) {
                        return Err(TypeError::Unsupported {
                            what: "`&` target must be a local variable or a chain of field accesses (e.g., `&x`, `&x.f`, `&x.f.g`)".into(),
                            span,
                        });
                    }
                    let inner_ty = self.check_expr(inner, env, ret_ty, in_class, loop_depth)?;
                    return Ok(Type::RawPtr {
                        is_const: false,
                        inner: Box::new(inner_ty),
                    });
                }
                let t = self.check_expr(inner, env, ret_ty, in_class, loop_depth)?;
                match op {
                    // Unary `-` is only meaningful on signed numerics.
                    UnOp::Neg if t.is_signed_int() || t.is_float() => Ok(t),
                    // Unary `+` is identity on any numeric.
                    UnOp::Pos if t.is_numeric() => Ok(t),
                    UnOp::Not if t == Type::Bool => Ok(t),
                    // Bit-not on any int (signed or unsigned).
                    UnOp::BitNot if t.is_int() => Ok(t),
                    // `~Flag.x` on a @flags enum yields a Flag value.
                    UnOp::BitNot
                        if matches!(&t, Type::Object(n) if self.enums.get(n).map(|s| s.flags).unwrap_or(false)) =>
                    {
                        Ok(t)
                    }
                    UnOp::AddrOf => unreachable!("handled above"),
                    _ => Err(TypeError::BadUnary { ty: t, span }),
                }
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let l = self.check_expr(lhs, env, ret_ty, in_class, loop_depth)?;
                let r = self.check_expr(rhs, env, ret_ty, in_class, loop_depth)?;
                // `@flags` enum: `Flag op Flag` for `|` `&` `^` returns Flag.
                if matches!(op, ilang_ast::BinOp::BitOr | ilang_ast::BinOp::BitAnd | ilang_ast::BinOp::BitXor) {
                    if let (Type::Object(ln), Type::Object(rn)) = (&l, &r) {
                        if ln == rn
                            && self.enums.get(ln).map(|s| s.flags).unwrap_or(false)
                        {
                            return Ok(l);
                        }
                    }
                }
                // Object identity comparison across an upcast: `p == q`
                // where one is a subclass of the other. The interpreter
                // does Rc-pointer equality regardless of static type;
                // the JIT compares the user pointer; both work fine.
                // bin_result only allows same-class identity, so we
                // special-case the subtype direction here before the
                // numeric paths below.
                if matches!(op, ilang_ast::BinOp::Eq | ilang_ast::BinOp::Ne) {
                    if let (Type::Object(a), Type::Object(b)) = (&l, &r) {
                        if a == b
                            || self.is_subclass(*a, *b)
                            || self.is_subclass(*b, *a)
                        {
                            return Ok(Type::Bool);
                        }
                    }
                    // Structural value equality on heap value containers:
                    // same-typed tuples, dynamic arrays, and optionals
                    // compare element-wise at runtime (the same
                    // value-equality `==` on string / enum already has).
                    // An optional pairs with `none` (inner `Any`).
                    let structural_eq_pair = match (&l, &r) {
                        (Type::Tuple(_), Type::Tuple(_)) => l == r,
                        (
                            Type::Array { elem: e1, fixed: None },
                            Type::Array { elem: e2, fixed: None },
                        ) => e1 == e2,
                        (Type::Optional(i1), Type::Optional(i2)) => {
                            i1 == i2 || **i1 == Type::Any || **i2 == Type::Any
                        }
                        _ => false,
                    };
                    if structural_eq_pair {
                        return Ok(Type::Bool);
                    }
                }
                // Enum-side promotion: `pub enum E: T { ... }`
                // values pair freely with a `T`-typed operand
                // (most common: `msg == WindowMessage.destroy`
                // where `msg: u32`). Promote the enum side to its
                // declared repr so the rest of the bin_result
                // logic handles it as a numeric comparison.
                let enum_repr = |t: &Type| -> Option<Type> {
                    if let Type::Object(name) = t {
                        if let Some(sig) = self.enums.get(name) {
                            return sig.repr.clone();
                        }
                    }
                    None
                };
                let l_repr = enum_repr(&l);
                let r_repr = enum_repr(&r);
                let (l, r) = match (l_repr, r_repr) {
                    (Some(lr), _) if lr == r => (lr, r),
                    (_, Some(rr)) if rr == l => (l, rr),
                    (Some(lr), Some(rr)) if lr == rr => (lr, rr),
                    // Repr enum vs an int literal that fits the repr:
                    // promote the enum to its repr AND adopt the literal,
                    // so `Msg.close == 18` works like `Msg.close == m`
                    // (where `m: u32`) already does.
                    (Some(lr), None) if lr.is_int() && numeric_literal_fits(rhs, &lr) => {
                        (lr.clone(), lr)
                    }
                    (None, Some(rr)) if rr.is_int() && numeric_literal_fits(lhs, &rr) => {
                        (rr.clone(), rr)
                    }
                    _ => (l, r),
                };
                // Literal-side adoption: when one operand is a
                // numeric literal whose value fits the other's
                // integer type, treat the literal as that type.
                // Lets `u32_var < 5000` and `u32_var != 0` work
                // without a manual `as u32`.
                let (l, r) = if l.is_int() && r.is_int()
                    && l.is_signed_int() != r.is_signed_int()
                {
                    if numeric_literal_fits(rhs, &l) {
                        (l.clone(), l)
                    } else if numeric_literal_fits(lhs, &r) {
                        (r.clone(), r)
                    } else {
                        (l, r)
                    }
                } else {
                    (l, r)
                };
                bin_result(*op, &l, &r).map_err(|e| attach_span(e, span))
            }
            ExprKind::Logical { op: _, lhs, rhs } => {
                let l = self.check_expr(lhs, env, ret_ty, in_class, loop_depth)?;
                let r = self.check_expr(rhs, env, ret_ty, in_class, loop_depth)?;
                if l != Type::Bool || r != Type::Bool {
                    return Err(TypeError::BadBinary {
                        lhs: l,
                        rhs: r,
                        span,
                    });
                }
                Ok(Type::Bool)
            }
            ExprKind::Call { callee, args } => {
                self.check_call_expr(callee, args, env, ret_ty, in_class, loop_depth, span)
            }
            ExprKind::Field { obj, name } => {
                self.check_field(obj, name, env, ret_ty, in_class, loop_depth, span)
            }
            ExprKind::MethodCall { obj, method, args } => {
                self.check_method_call(obj, method, args, env, ret_ty, in_class, loop_depth, span)
            }
            ExprKind::New { class, type_args, args, init_method } => {
                self.check_new(class, type_args, args, init_method, env, ret_ty, in_class, loop_depth, span)
            }
            ExprKind::Block(b) => self.check_block(b, env, ret_ty, in_class, loop_depth),
            ExprKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.check_if_expr(cond, then_branch, else_branch, env, ret_ty, in_class, loop_depth, span)
            }
            ExprKind::While { cond, body } => {
                let c = self.check_expr(cond, env, ret_ty, in_class, loop_depth)?;
                if c != Type::Bool {
                    return Err(TypeError::Mismatch {
                        expected: Type::Bool,
                        got: c,
                        span: cond.span,
                    });
                }
                self.loop_stack.borrow_mut().push(LoopFrame::Other);
                let body_res = self.check_block(body, env, ret_ty, in_class, loop_depth + 1);
                self.loop_stack.borrow_mut().pop();
                // While body is a statement — any trailing
                // expression value is silently discarded.
                let _body_ty = body_res?;
                Ok(Type::Unit)
            }
            ExprKind::Loop { body } => {
                self.loop_stack.borrow_mut().push(LoopFrame::Loop(None));
                let body_res = self.check_block(body, env, ret_ty, in_class, loop_depth + 1);
                let frame = self.loop_stack.borrow_mut().pop();
                // Loop body is a statement — the trailing expression
                // value is discarded (only `break v` produces the
                // loop's overall value).
                let _body_ty = body_res?;
                // The loop's own type is the unified break-value type. A
                // bare `break` makes it `Unit`. With NO `break` at all the
                // loop never falls through to a value — it exits only via
                // `return` (or runs forever), so it DIVERGES: type it as
                // the fn's return type, the same way `return` and an
                // all-arms-return `match` do (第144弾). Without this,
                // `fn f(): i64 { loop { ...; return v } }` and a bare
                // `fn f(): i64 { loop {} }` were wrongly rejected as
                // "body produces ()".
                let break_ty = match frame {
                    Some(LoopFrame::Loop(Some(t))) => t,
                    Some(LoopFrame::Loop(None)) => ret_ty.cloned().unwrap_or(Type::Unit),
                    _ => Type::Unit,
                };
                self.loop_break_type
                    .borrow_mut()
                    .insert(span, break_ty.clone());
                Ok(break_ty)
            }
            ExprKind::ForIn { var, iter, body } => {
                self.check_for_in(var, iter, body, env, ret_ty, in_class, loop_depth, span)
            }
            ExprKind::Break(value) => {
                if loop_depth == 0 {
                    return Err(TypeError::BreakOutsideLoop { span });
                }
                let val_ty = match value {
                    Some(e) => Some((self.check_expr(e, env, ret_ty, in_class, loop_depth)?, e.span)),
                    None => None,
                };
                // The innermost loop frame governs whether `break v` is
                // allowed and (for `loop`) collects its value type.
                let mut stack = self.loop_stack.borrow_mut();
                let frame = stack.last_mut().expect("loop_depth > 0 implies non-empty stack");
                match frame {
                    LoopFrame::Other => {
                        if value.is_some() {
                            return Err(TypeError::Unsupported {
                                what: "`break value` is only allowed inside `loop` (not `while` / `for`)".into(),
                                span,
                            });
                        }
                    }
                    LoopFrame::Loop(acc) => {
                        let new_ty = val_ty.as_ref().map(|(t, _)| t.clone()).unwrap_or(Type::Unit);
                        match acc {
                            None => *acc = Some(new_ty),
                            Some(prev) => {
                                if *prev != new_ty {
                                    // Same fallback as if/else branch
                                    // unification: try to merge generic
                                    // holes, then accept a bare numeric
                                    // literal that fits the prior type.
                                    if let Some(merged) =
                                        merge_generic_with_holes(prev, &new_ty)
                                    {
                                        *prev = merged;
                                    } else if value
                                        .as_deref()
                                        .is_some_and(|e| numeric_literal_fits(e, prev))
                                    {
                                        // keep `prev` — current break's
                                        // literal coerces into it.
                                    } else {
                                        return Err(TypeError::Mismatch {
                                            expected: prev.clone(),
                                            got: new_ty,
                                            span: val_ty.map(|(_, s)| s).unwrap_or(span),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(Type::Unit)
            }
            ExprKind::Continue => {
                if loop_depth == 0 {
                    return Err(TypeError::ContinueOutsideLoop { span });
                }
                Ok(Type::Unit)
            }
            ExprKind::Range { .. } => Err(TypeError::Unsupported {
                what: "range expression `a..b` is only valid as a `for-in` iterator".into(),
                span,
            }),
            ExprKind::Return(value) => {
                // Top-level `return` is allowed as an early-exit
                // from the program. Carrying a value is rejected
                // there — the program's value is its tail expr,
                // not a `return value`.
                let Some(expected) = ret_ty.cloned() else {
                    if value.is_some() {
                        return Err(TypeError::Unsupported {
                            what: "top-level `return` cannot carry a value (the program's value is its tail expression)".into(),
                            span,
                        });
                    }
                    return Ok(Type::Unit);
                };
                match value {
                    Some(v) => {
                        let vt = self.check_expr(v, env, ret_ty, in_class, loop_depth)?;
                        if !self.value_assignable(v, &vt, &expected) {
                            return Err(TypeError::Mismatch {
                                expected,
                                got: vt,
                                span: v.span,
                            });
                        }
                    }
                    None => {
                        if !matches!(expected, Type::Unit) {
                            return Err(TypeError::Mismatch {
                                expected,
                                got: Type::Unit,
                                span,
                            });
                        }
                    }
                }
                // `return` diverges — control never continues past it.
                // We pretend the expression has the function's return
                // type so a body ending in `return X` and a non-else
                // `if cond { return X }` both type-check without
                // needing a separate Never type.
                Ok(expected)
            }
            ExprKind::Assign { target, value } => {
                if let Some(var_ty) = env.get(target).cloned() {
                    if self.const_names.borrow().contains(target)
                        || self.top_level_consts.borrow().contains(target)
                    {
                        return Err(TypeError::Unsupported {
                            what: format!(
                                "cannot assign to `{target}` — it is bound by `const` (one-time assignment)"
                            ),
                            span,
                        });
                    }
                    let v_ty = self.check_expr(value, env, ret_ty, in_class, loop_depth)?;
                    // Refine a generic enum constructor's stashed type
                    // args from the binding's type (a `r = Result.err(..)`
                    // reassign needs the same treatment as `let r: T = ..`).
                    self.refine_enum_ctor_args(value, &var_ty);
                    if !self.value_assignable(value, &v_ty, &var_ty) {
                        return Err(TypeError::Mismatch {
                            expected: var_ty,
                            got: v_ty,
                            span: value.span,
                        });
                    }
                    return Ok(Type::Unit);
                }
                if let Some(class_name) = in_class {
                    if let Some(cls) = self.classes.get(&class_name) {
                        if let Some(field_ty) = cls.fields.get(target).cloned() {
                            let v_ty = self.check_expr(value, env, ret_ty, in_class, loop_depth)?;
                            // Bare implicit-`this` field write — refine the
                            // enum ctor from the field's declared type, as
                            // the explicit `this.f = ..` path does.
                            self.refine_enum_ctor_args(value, &field_ty);
                            if !self.value_assignable(value, &v_ty, &field_ty) {
                                return Err(TypeError::Mismatch {
                                    expected: field_ty,
                                    got: v_ty,
                                    span: value.span,
                                });
                            }
                            return Ok(Type::Unit);
                        }
                    }
                }
                Err(TypeError::UndefinedVariable {
                    name: target.clone(),
                    span,
                })
            }
            ExprKind::Array(elements) => {
                self.check_array(elements, env, ret_ty, in_class, loop_depth, span)
            }
            ExprKind::Tuple(elements) => {
                let mut tys = Vec::with_capacity(elements.len());
                for e in elements {
                    tys.push(self.check_expr(e, env, ret_ty, in_class, loop_depth)?);
                }
                Ok(Type::Tuple(tys.into()))
            }
            ExprKind::MapLit(entries) => {
                self.check_map_lit(entries, env, ret_ty, in_class, loop_depth, span)
            }
            ExprKind::Index { obj, index } => {
                self.check_index(obj, index, env, ret_ty, in_class, loop_depth, span)
            }
            ExprKind::AssignIndex { obj, index, value } => {
                self.check_assign_index(obj, index, value, env, ret_ty, in_class, loop_depth, span)
            }
            ExprKind::FnExpr { params, ret, body } => {
                self.check_fn_expr(params, ret, body, env, ret_ty, in_class, loop_depth, span)
            }
            ExprKind::Cast { expr: inner, ty } => {
                self.check_cast(inner, ty, env, ret_ty, in_class, loop_depth, span)
            }
            ExprKind::TypeTest { expr: inner, ty } => {
                self.check_expr(inner, env, ret_ty, in_class, loop_depth)?;
                self.validate_type(ty, span, &[])?;
                Ok(Type::Bool)
            }
            ExprKind::TypeDowncast { expr: inner, ty } => {
                self.check_expr(inner, env, ret_ty, in_class, loop_depth)?;
                self.validate_type(ty, span, &[])?;
                Ok(Type::Optional(Box::new(ty.clone())))
            }
            ExprKind::AssignField { obj, field, value, is_init } => {
                self.check_assign_field(obj, field, value, is_init, env, ret_ty, in_class, loop_depth, span)
            }
            ExprKind::None => Ok(Type::Optional(Box::new(Type::Any))),
            ExprKind::Some(inner) => {
                let it = self.check_expr(inner, env, ret_ty, in_class, loop_depth)?;
                Ok(Type::Optional(Box::new(it)))
            }
            ExprKind::Await(inner) => {
                let it = self.check_expr(inner, env, ret_ty, in_class, loop_depth)?;
                // `await expr` requires `Promise<T>` and evaluates to T.
                let inner_ty = match it {
                    Type::Generic(g)
                        if g.base.as_str() == "Promise" && g.args.len() == 1 =>
                    {
                        g.args[0].clone()
                    }
                    other => {
                        return Err(TypeError::Mismatch {
                            expected: Type::generic("Promise", vec![Type::Any]),
                            got: other,
                            span,
                        });
                    }
                };
                // Async-fn bodies are rewritten to a state machine
                // before the type checker sees them, so an `await`
                // reaching here means it appears outside an `async
                // fn` body. Surface that directly.
                let _ = inner_ty;
                Err(TypeError::Unsupported {
                    what:
                        "`await` is only allowed inside an `async fn` body \
                         (top-level / sync fn / lambda bodies aren't \
                         covered). Either call `.then(fn(v) { ... })` on \
                         the promise, or wrap the awaiting code in an \
                         `async fn run() { ... await ... }` and kick it \
                         with `let _ = run()`."
                            .to_string(),
                    span,
                })
            }
            ExprKind::IfLet {
                name,
                expr,
                then_branch,
                else_branch,
            } => {
                self.check_if_let(name, expr, then_branch, else_branch, env, ret_ty, in_class, loop_depth, span)
            }
            ExprKind::EnumCtor {
                enum_name,
                variant,
                args,
            } => {
                self.check_enum_ctor(enum_name, variant, args, env, ret_ty, in_class, loop_depth, span)
            }
            ExprKind::Match { scrutinee, arms } => {
                self.check_match_expr(scrutinee, arms, env, ret_ty, in_class, loop_depth, span)
            }
            ExprKind::Template { parts } => {
                // Backtick-quoted template literal. Each interpolated
                // expression is type-checked but its concrete type is
                // free — the lowering stage emits the appropriate
                // `$fmt.*` conversion per type. Result type is always
                // `string`. Literal chunks contribute nothing to type
                // checking.
                for part in parts.iter() {
                    if let ilang_ast::TemplatePart::Expr(e) = part {
                        self.check_expr(e, env, ret_ty, in_class, loop_depth)?;
                    }
                }
                Ok(Type::Str)
            }
        }
    }

}
