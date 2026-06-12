//! `ExprKind::Match` / `ExprKind::EnumCtor` — pattern-matching
//! and enum-constructor lowering, extracted from `expr/mod.rs`.

#![allow(unused_imports)]

use std::collections::{HashMap, HashSet};

use ilang_ast::{
    Block, ClassDecl, CtorArgs, EnumDecl, Expr, ExprKind, FieldDecl, FnDecl, Item, Param,
    PatternBindings, PatternKind, Program, Span, Stmt, StmtKind, Symbol, Type, UnOp,
    VariantPayload,
};

use crate::error::TypeError;
use crate::ops::{assignable, bin_result, int_literal_fits};

use super::arm_body_diverges;
use super::super::*;

impl TypeChecker {
    pub(super) fn check_match_expr(
        &self,
        scrutinee: &Expr, arms: &[ilang_ast::MatchArm],
        env: &Vars,
        ret_ty: Option<&Type>,
        in_class: Option<Symbol>,
        loop_depth: u32,
        span: Span,
    ) -> Result<Type, TypeError> {
        let st = self.check_expr(scrutinee, env, ret_ty, in_class, loop_depth)?;
        // Match on a primitive (integer / bool / string)
        // is allowed, with `IntLit` / `BoolLit` / `StrLit`
        // patterns. Bool literals appear as
        // `Variant{name: "true"|"false"}` from the parser,
        // which we treat as `BoolLit` here.
        if st.is_numeric() || st == Type::Bool || st == Type::Str {
            return self.check_match_primitive(&st, arms, span, env, ret_ty, in_class, loop_depth);
        }
        // `match opt { some(v) { ... } none { ... } }` on
        // `Optional<T>`. Treat exactly like a 2-variant enum:
        // accept `some(name)` (binds name to T), `none`, and
        // `_`; require both variants covered (or a wildcard).
        if let Type::Optional(inner) = &st {
            if matches!(**inner, Type::Any) {
                return Err(TypeError::Mismatch {
                    expected: Type::Optional(Box::new(Type::Any)),
                    got: st.clone(),
                    span: scrutinee.span,
                });
            }
            return self.check_match_optional(
                (**inner).clone(),
                arms,
                env,
                ret_ty,
                in_class,
                loop_depth,
                span,
            );
        }
        let (enum_name, scrut_args) = match &st {
            Type::Object(name) if self.enums.contains_key(name) => {
                (name.clone(), Vec::<Type>::new())
            }
            Type::Generic(g) if self.enums.contains_key(&g.base) => {
                (g.base.clone(), g.args.to_vec())
            }
            _ => {
                return Err(TypeError::Mismatch {
                    expected: Type::Object("<enum>".into()),
                    got: st,
                    span: scrutinee.span,
                });
            }
        };
        let sig = self.enums[&enum_name].clone();
        let enum_params = sig.type_params.clone();
        let mut covered: std::collections::HashSet<Symbol> =
            std::collections::HashSet::new();
        let mut has_wildcard = false;
        let mut result_ty: Option<Type> = None;
        for arm in arms {
            if has_wildcard {
                return Err(TypeError::Unsupported {
                    what: "match arm after wildcard `_` is unreachable".into(),
                    span: arm.span,
                });
            }
            let mut arm_env = env.clone();
            let arm_kind_span = arm.pattern.span;
            match &arm.pattern.kind {
                PatternKind::Wildcard => {
                    has_wildcard = true;
                }
                PatternKind::IntLit(_)
                | PatternKind::IntRange { .. }
                | PatternKind::BoolLit(_)
                | PatternKind::StrLit(_) => {
                    return Err(TypeError::Unsupported {
                        what: format!(
                            "literal pattern not allowed when matching enum {enum_name:?}"
                        ),
                        span: arm_kind_span,
                    });
                }
                PatternKind::Variant {
                    enum_name: pat_enum,
                    variant,
                    bindings,
                } => {
                    // Short form (`Variant ...` without `Enum::`)
                    // borrows the scrutinee's enum name. Long
                    // form must match it exactly.
                    if let Some(pe) = pat_enum {
                        if pe != &enum_name {
                            return Err(TypeError::Mismatch {
                                expected: Type::Object(enum_name.clone()),
                                got: Type::Object(pe.clone()),
                                span: arm_kind_span,
                            });
                        }
                    }
                    let v = sig
                        .variants
                        .iter()
                        .find(|v| v.name == *variant)
                        .ok_or_else(|| TypeError::Unsupported {
                            what: format!(
                                "enum {enum_name:?} has no variant {variant:?}"
                            ),
                            span: arm_kind_span,
                        })?;
                    if !covered.insert(variant.clone()) {
                        return Err(TypeError::Unsupported {
                            what: format!("duplicate match arm for {variant:?}"),
                            span: arm_kind_span,
                        });
                    }
                    // Check binding shape matches and add bindings.
                    // Generic enums: substitute the scrutinee's
                    // concrete type args into each parametric
                    // payload type before binding the name.
                    match (&v.payload, bindings) {
                        (VariantPayloadSig::Unit, PatternBindings::Unit) => {}
                        (
                            VariantPayloadSig::Tuple(tys),
                            PatternBindings::Tuple(names),
                        ) => {
                            if tys.len() != names.len() {
                                return Err(TypeError::ArityMismatch {
                                    name: Symbol::intern(&format!("{enum_name}::{variant}")),
                                    expected: tys.len(),
                                    got: names.len(),
                                    span: arm_kind_span,
                                });
                            }
                            for (n, t) in names.iter().zip(tys.iter()) {
                                if n != "_" {
                                    let bind_ty =
                                        subst_type(t, &enum_params, &scrut_args);
                                    arm_env.insert(n.clone(), bind_ty);
                                }
                            }
                        }
                        (
                            VariantPayloadSig::Struct(fields),
                            PatternBindings::Struct(pairs),
                        ) => {
                            for (fname, bname) in pairs {
                                let fty = fields
                                    .iter()
                                    .find(|(n, _)| n == fname)
                                    .map(|(_, t)| t.clone())
                                    .ok_or_else(|| TypeError::UnknownField {
                                        class: Symbol::intern(&format!("{enum_name}::{variant}")),
                                        field: fname.clone(),
                                        span: arm_kind_span,
                                    })?;
                                if bname != "_" {
                                    let bind_ty =
                                        subst_type(&fty, &enum_params, &scrut_args);
                                    arm_env.insert(bname.clone(), bind_ty);
                                }
                            }
                        }
                        _ => {
                            return Err(TypeError::Unsupported {
                                what: format!(
                                    "pattern shape for {enum_name}::{variant} doesn't match its declaration"
                                ),
                                span: arm_kind_span,
                            });
                        }
                    }
                }
            }
            let bt = self.check_expr(&arm.body, &arm_env, ret_ty, in_class, loop_depth)?;
            // An arm whose body unconditionally diverges
            // (early `return` / `break` / `continue`) doesn't
            // contribute a value to the match result. Skip
            // unifying its (fictional) type. Without this,
            // the `?` desugaring's err-arm — literally
            // `return Result.err(e)` — types as the enclosing
            // fn's full return type and conflicts with the
            // ok-arm's payload type.
            if !arm_body_diverges(&arm.body) {
                result_ty = Some(match result_ty {
                    None => bt,
                    Some(prev) => self.unify_branch_obj(prev, bt, arm.body.span)?,
                });
            }
        }
        if !has_wildcard {
            let total = sig.variants.len();
            if covered.len() != total {
                let missing: Vec<_> = sig
                    .variants
                    .iter()
                    .filter(|v| !covered.contains(&v.name))
                    .map(|v| v.name.as_str())
                    .collect::<Vec<_>>();
                return Err(TypeError::Unsupported {
                    what: format!(
                        "non-exhaustive match on {enum_name}: missing {}",
                        missing.join(", ")
                    ),
                    span,
                });
            }
        }
        Ok(result_ty.unwrap_or(Type::Unit))
    }
}


impl TypeChecker {
    fn check_match_optional(
        &self,
        inner: Type,
        arms: &[ilang_ast::MatchArm],
        env: &Vars,
        ret_ty: Option<&Type>,
        in_class: Option<Symbol>,
        loop_depth: u32,
        span: Span,
    ) -> Result<Type, TypeError> {
        let mut has_some = false;
        let mut has_none = false;
        let mut has_wildcard = false;
        let mut result_ty: Option<Type> = None;
        for arm in arms {
            if has_wildcard {
                return Err(TypeError::Unsupported {
                    what: "match arm after wildcard `_` is unreachable".into(),
                    span: arm.span,
                });
            }
            let mut arm_env = env.clone();
            let arm_kind_span = arm.pattern.span;
            match &arm.pattern.kind {
                PatternKind::Wildcard => {
                    has_wildcard = true;
                }
                PatternKind::Variant {
                    enum_name: pat_enum,
                    variant,
                    bindings,
                } => {
                    if pat_enum.is_some() {
                        return Err(TypeError::Unsupported {
                            what: "qualified variant pattern not allowed for Optional"
                                .into(),
                            span: arm_kind_span,
                        });
                    }
                    match variant.as_str() {
                        "some" => {
                            if has_some {
                                return Err(TypeError::Unsupported {
                                    what: "duplicate match arm for some".into(),
                                    span: arm_kind_span,
                                });
                            }
                            has_some = true;
                            match bindings {
                                PatternBindings::Tuple(names) if names.len() == 1 => {
                                    let n = &names[0];
                                    if n.as_str() != "_" {
                                        arm_env.insert(n.clone(), inner.clone());
                                    }
                                }
                                _ => {
                                    return Err(TypeError::Unsupported {
                                        what: "`some` pattern needs exactly one binding: `some(name)`"
                                            .into(),
                                        span: arm_kind_span,
                                    });
                                }
                            }
                        }
                        "none" => {
                            if has_none {
                                return Err(TypeError::Unsupported {
                                    what: "duplicate match arm for none".into(),
                                    span: arm_kind_span,
                                });
                            }
                            has_none = true;
                            if !matches!(bindings, PatternBindings::Unit) {
                                return Err(TypeError::Unsupported {
                                    what: "`none` pattern takes no bindings".into(),
                                    span: arm_kind_span,
                                });
                            }
                        }
                        other => {
                            return Err(TypeError::Unsupported {
                                what: format!(
                                    "Optional has no variant {other:?} (use `some(x)` / `none`)"
                                ),
                                span: arm_kind_span,
                            });
                        }
                    }
                }
                _ => {
                    return Err(TypeError::Unsupported {
                        what: "literal pattern not allowed when matching Optional".into(),
                        span: arm_kind_span,
                    });
                }
            }
            let bt = self.check_expr(&arm.body, &arm_env, ret_ty, in_class, loop_depth)?;
            if !arm_body_diverges(&arm.body) {
                result_ty = Some(match result_ty {
                    None => bt,
                    Some(prev) => self.unify_branch_obj(prev, bt, arm.body.span)?,
                });
            }
        }
        if !has_wildcard && !(has_some && has_none) {
            let mut missing: Vec<&str> = Vec::new();
            if !has_some {
                missing.push("some");
            }
            if !has_none {
                missing.push("none");
            }
            return Err(TypeError::Unsupported {
                what: format!(
                    "non-exhaustive match on Optional: missing {}",
                    missing.join(", ")
                ),
                span,
            });
        }
        Ok(result_ty.unwrap_or(Type::Unit))
    }
}

impl TypeChecker {
    pub(super) fn check_enum_ctor(
        &self,
        enum_name: &Symbol, variant: &Symbol, args: &ilang_ast::CtorArgs,
        env: &Vars,
        ret_ty: Option<&Type>,
        in_class: Option<Symbol>,
        loop_depth: u32,
        span: Span,
    ) -> Result<Type, TypeError> {
        let sig = self.enums.get(enum_name).cloned().ok_or_else(|| {
            TypeError::UndefinedClass {
                name: enum_name.clone(),
                span,
            }
        })?;
        let v = sig.variants.iter().find(|v| v.name == *variant).ok_or_else(|| {
            TypeError::Unsupported {
                what: format!("enum {enum_name:?} has no variant {variant:?}"),
                span,
            }
        })?;
        let type_params = sig.type_params.clone();
        // First pass: gather arg types, infer type-parameter
        // bindings from the (parametric payload type, arg type)
        // pairs. Bindings absent here stay as `Any`, to be
        // refined by an outer annotation.
        let mut bindings: HashMap<Symbol, Type> = HashMap::new();
        let mut arg_tys_tuple: Vec<Type> = Vec::new();
        let mut arg_tys_struct: Vec<(Symbol, Type)> = Vec::new();
        match (&v.payload, args) {
            (VariantPayloadSig::Unit, CtorArgs::Unit) => {}
            (VariantPayloadSig::Tuple(tys), CtorArgs::Tuple(elems)) => {
                if tys.len() != elems.len() {
                    return Err(TypeError::ArityMismatch {
                        name: Symbol::intern(&format!("{enum_name}::{variant}")),
                        expected: tys.len(),
                        got: elems.len(),
                        span,
                    });
                }
                for (e, t) in elems.iter().zip(tys.iter()) {
                    let et = self.check_expr(e, env, ret_ty, in_class, loop_depth)?;
                    collect_type_var_bindings(t, &et, &mut bindings);
                    arg_tys_tuple.push(et);
                }
            }
            (VariantPayloadSig::Struct(fields), CtorArgs::Struct(provided)) => {
                if provided.len() != fields.len() {
                    return Err(TypeError::ArityMismatch {
                        name: Symbol::intern(&format!("{enum_name}::{variant}")),
                        expected: fields.len(),
                        got: provided.len(),
                        span,
                    });
                }
                for (fname, fty) in fields {
                    let supplied = provided.iter().find(|(n, _)| n == fname).ok_or_else(
                        || TypeError::UnknownField {
                            class: Symbol::intern(&format!("{enum_name}::{variant}")),
                            field: fname.clone(),
                            span,
                        },
                    )?;
                    let st = self.check_expr(
                        &supplied.1,
                        env,
                        ret_ty,
                        in_class,
                        loop_depth,
                    )?;
                    collect_type_var_bindings(fty, &st, &mut bindings);
                    arg_tys_struct.push((fname.clone(), st));
                }
            }
            _ => {
                return Err(TypeError::Unsupported {
                    what: format!(
                        "constructor shape for {enum_name}::{variant} doesn't match its declaration"
                    ),
                    span,
                });
            }
        }
        // Build inferred type-arg vector (Any for unsolved).
        let inferred_args: Vec<Type> = type_params
            .iter()
            .map(|p| bindings.get(p).cloned().unwrap_or(Type::Any))
            .collect();
        for (p, t) in type_params.iter().zip(inferred_args.iter()) {
            self.reject_fixed_heap_type_arg(p.as_str(), t, span)?;
        }
        // Stash for the JIT enum-monomorphization pass. Args
        // may still contain TypeVars when the call sits inside
        // another generic context — that's resolved at
        // expansion time. Always recorded (even for non-generic
        // enums) since the cost is trivial.
        if !type_params.is_empty() {
            self.enum_ctor_type_args
                .borrow_mut()
                .insert(span, (enum_name.clone(), inferred_args.clone()));
        }
        // Validate each arg against the substituted payload type.
        match (&v.payload, args) {
            (VariantPayloadSig::Unit, _) => {}
            (VariantPayloadSig::Tuple(tys), CtorArgs::Tuple(elems)) => {
                for ((e, t), et) in elems.iter().zip(tys.iter()).zip(arg_tys_tuple.iter()) {
                    let actual = subst_type(t, &type_params, &inferred_args);
                    if !self.value_assignable(e, et, &actual) {
                        return Err(TypeError::Mismatch {
                            expected: actual,
                            got: et.clone(),
                            span: e.span,
                        });
                    }
                }
            }
            (VariantPayloadSig::Struct(fields), CtorArgs::Struct(provided)) => {
                for (fname, fty) in fields {
                    let supplied = provided.iter().find(|(n, _)| n == fname).unwrap();
                    let st = arg_tys_struct
                        .iter()
                        .find(|(n, _)| n == fname)
                        .map(|(_, t)| t.clone())
                        .unwrap();
                    let actual = subst_type(fty, &type_params, &inferred_args);
                    if !self.value_assignable(&supplied.1, &st, &actual) {
                        return Err(TypeError::Mismatch {
                            expected: actual,
                            got: st,
                            span: supplied.1.span,
                        });
                    }
                }
            }
            _ => {}
        }
        Ok(if type_params.is_empty() {
            Type::Object(enum_name.clone())
        } else {
            Type::generic(enum_name.clone(), inferred_args)
        })
    }
}

impl TypeChecker {
    pub(super) fn check_for_in(
        &self,
        var: &Symbol, iter: &Expr, body: &Block,
        env: &Vars,
        ret_ty: Option<&Type>,
        in_class: Option<Symbol>,
        loop_depth: u32,
        _span: Span,
    ) -> Result<Type, TypeError> {
        // Range iter: check both endpoints are integer types of
        // a single common int type, bind `var` to that type.
        let elem = if let ExprKind::Range { start, end, .. } = &iter.kind {
            let start = match start {
                Some(s) => s,
                None => {
                    return Err(TypeError::Unsupported {
                        what: "for-in range needs a start (`..N` is not iterable; use `0..N`)".into(),
                        span: iter.span,
                    });
                }
            };
            let st = self.check_expr(start, env, ret_ty, in_class, loop_depth)?;
            if !st.is_int() {
                return Err(TypeError::Mismatch {
                    expected: Type::I64,
                    got: st,
                    span: start.span,
                });
            }
            if let Some(end) = end {
                let et = self.check_expr(end, env, ret_ty, in_class, loop_depth)?;
                if !et.is_int() {
                    return Err(TypeError::Mismatch {
                        expected: st.clone(),
                        got: et,
                        span: end.span,
                    });
                }
                if st != et {
                    if numeric_literal_fits(start, &et) {
                        et
                    } else if numeric_literal_fits(end, &st) {
                        st
                    } else {
                        return Err(TypeError::Mismatch {
                            expected: st,
                            got: et,
                            span: end.span,
                        });
                    }
                } else {
                    st
                }
            } else {
                // Open-ended `start..` — iter type is just
                // start's type. Body must `break` to exit.
                st
            }
        } else {
            let it = self.check_expr(iter, env, ret_ty, in_class, loop_depth)?;
            match &it {
                Type::Array { elem, .. } => (**elem).clone(),
                other => {
                    return Err(TypeError::Mismatch {
                        expected: Type::Array {
                            elem: Box::new(Type::Any),
                            fixed: None,
                        },
                        got: other.clone(),
                        span: iter.span,
                    });
                }
            }
        };
        let mut inner = env.clone();
        inner.insert(var.clone(), elem);
        self.loop_stack.borrow_mut().push(LoopFrame::Other);
        let body_res =
            self.check_block(body, &inner, ret_ty, in_class, loop_depth + 1);
        self.loop_stack.borrow_mut().pop();
        // For body is a statement — the trailing expression value
        // is silently discarded.
        let _body_ty = body_res?;
        Ok(Type::Unit)
    }
}


impl TypeChecker {
    pub(super) fn check_if_let(
        &self,
        name: &Symbol, expr: &Expr, then_branch: &Block, else_branch: &Option<Box<Expr>>,
        env: &Vars,
        ret_ty: Option<&Type>,
        in_class: Option<Symbol>,
        loop_depth: u32,
        span: Span,
    ) -> Result<Type, TypeError> {
        let scrut_ty = self.check_expr(expr, env, ret_ty, in_class, loop_depth)?;
        let inner = match &scrut_ty {
            Type::Optional(t) => (**t).clone(),
            _ => {
                return Err(TypeError::Mismatch {
                    expected: Type::Optional(Box::new(Type::Any)),
                    got: scrut_ty,
                    span: expr.span,
                });
            }
        };
        // Inner must be concrete for the binding to be useful;
        // we reject `if let some(x) = none` because the type of
        // x would be `Any`.
        if matches!(inner, Type::Any) {
            return Err(TypeError::Mismatch {
                expected: Type::Optional(Box::new(Type::Any)),
                got: scrut_ty,
                span: expr.span,
            });
        }
        let mut then_env = env.clone();
        then_env.insert(name.clone(), inner);
        let then_ty = self.check_block(then_branch, &then_env, ret_ty, in_class, loop_depth)?;
        if let Some(eb) = else_branch {
            let else_ty = self.check_expr(eb, env, ret_ty, in_class, loop_depth)?;
            // Class subtype upcast: if both branches produce
            // Object types and one is a subclass of the
            // other (or they share a common ancestor), the
            // join is the parent. Mirrors the regular
            // if/else path's rule.
            let class_join = match (&then_ty, &else_ty) {
                (Type::Object(t), Type::Object(e)) => {
                    self.common_ancestor(*t, *e).map(Type::Object)
                }
                _ => None,
            };
            // Pick the unifying type: if either branch is Unit, the
            // overall expr is Unit (statement-style); otherwise the
            // two branches must agree.
            if matches!(then_ty, Type::Unit) || matches!(else_ty, Type::Unit) {
                Ok(Type::Unit)
            } else if assignable(&else_ty, &then_ty) {
                Ok(then_ty)
            } else if assignable(&then_ty, &else_ty) {
                Ok(else_ty)
            } else if let Some(merged) = merge_generic_with_holes(&then_ty, &else_ty) {
                // Each branch fixed a different generic hole
                // (e.g. `Result<i64, Any>` and `Result<Any, string>`)
                // — merge to the more specific shape. Mirrors the
                // regular if/else path.
                Ok(merged)
            } else if let Some(joined) = class_join {
                Ok(joined)
            } else if let Some(t) = then_branch.tail.as_deref() {
                if numeric_literal_fits(t, &else_ty) {
                    Ok(else_ty)
                } else if numeric_literal_fits(eb, &then_ty) {
                    Ok(then_ty)
                } else {
                    Err(TypeError::Mismatch {
                        expected: then_ty,
                        got: else_ty,
                        span,
                    })
                }
            } else if numeric_literal_fits(eb, &then_ty) {
                Ok(then_ty)
            } else {
                Err(TypeError::Mismatch {
                    expected: then_ty,
                    got: else_ty,
                    span,
                })
            }
        } else {
            // No else: the result is Unit even if then has a value.
            Ok(Type::Unit)
        }
    }
}

impl TypeChecker {
    pub(super) fn check_if_expr(
        &self,
        cond: &Expr, then_branch: &Block, else_branch: &Option<Box<Expr>>,
        env: &Vars,
        ret_ty: Option<&Type>,
        in_class: Option<Symbol>,
        loop_depth: u32,
        _span: Span,
    ) -> Result<Type, TypeError> {
        let c = self.check_expr(cond, env, ret_ty, in_class, loop_depth)?;
        if c != Type::Bool {
            return Err(TypeError::Mismatch {
                expected: Type::Bool,
                got: c,
                span: cond.span,
            });
        }
        let then_ty = self.check_block(then_branch, env, ret_ty, in_class, loop_depth)?;
        match else_branch {
            None => {
                // No else: the expression evaluates to () regardless
                // of the then-branch's type (any value would be
                // discarded). Mirrors `if let some(...)` and matches
                // the JS-style intent of "do this conditionally".
                Ok(Type::Unit)
            }
            Some(else_e) => {
                let else_ty = self.check_expr(else_e, env, ret_ty, in_class, loop_depth)?;
                if then_ty == else_ty {
                    return Ok(then_ty);
                }
                // Generic types (e.g. `Result<T, E>`) where each
                // arm fixed a different type parameter need to
                // be merged into the more specific shape — e.g.
                // `Result<i64, Any>` and `Result<Any, string>`
                // unify to `Result<i64, string>`.
                if let Some(merged) = merge_generic_with_holes(&then_ty, &else_ty) {
                    return Ok(merged);
                }
                // Class subtype upcast: if both branches
                // produce Object types and they share a
                // common ancestor, the whole `if` takes
                // that ancestor. (Restricted to
                // Object↔Object so `i64 ↔ f64` still errors
                // per the no-implicit-numeric-widening rule
                // above.)
                if let (Type::Object(a), Type::Object(b)) =
                    (&then_ty, &else_ty)
                {
                    if let Some(anc) = self.common_ancestor(*a, *b) {
                        return Ok(Type::Object(anc));
                    }
                }
                // Optional unification: a bare `none` arm has
                // inferred type `any?`, while a `some(v)` arm
                // has `T?`. Prefer the concrete side at every
                // Optional layer so nested cases like
                // `if cond { some(some(v)) } else { none }`
                // (`some(none)` etc.) infer as the user
                // expects. Object-typed inners are joined via
                // the existing common-ancestor rule.
                if let Some(merged) = self.unify_optional_branches(&then_ty, &else_ty) {
                    return Ok(merged);
                }
                // Rust 流: 暗黙の数値拡張は禁止 (i64 と f64 を
                // ぶつけたらエラー)。例外として、片方のアームの末尾式
                // が「素の数値リテラル」 (整数/浮動小数、単項マイナス
                // 込み) で、もう一方の型に収まるときだけ受け入れる。
                let then_tail = then_branch.tail.as_deref();
                if let Some(t) = then_tail {
                    if numeric_literal_fits(t, &else_ty) {
                        return Ok(else_ty);
                    }
                }
                if numeric_literal_fits(else_e, &then_ty) {
                    return Ok(then_ty);
                }
                Err(TypeError::Mismatch {
                    expected: then_ty,
                    got: else_ty,
                    span: else_e.span,
                })
            }
        }
    }

    /// Walk down an Optional chain and pick the concrete side at
    /// each layer. `Any` (the type a bare `none` literal carries)
    /// yields to the sibling's more concrete type; Object inners
    /// collapse to their common ancestor when both are object
    /// types. Returns `None` when no consistent merge exists
    /// (e.g. `Optional<i64>` ↔ `Optional<string>`), which lets
    /// the caller fall through to the standard mismatch error.
    pub(super) fn unify_optional_branches(
        &self,
        a: &Type,
        b: &Type,
    ) -> Option<Type> {
        if a == b {
            return Some(a.clone());
        }
        match (a, b) {
            (Type::Any, _) => Some(b.clone()),
            (_, Type::Any) => Some(a.clone()),
            (Type::Optional(ia), Type::Optional(ib)) => {
                let inner = self.unify_optional_branches(ia, ib)?;
                Some(Type::Optional(Box::new(inner)))
            }
            (Type::Object(ca), Type::Object(cb)) => {
                self.common_ancestor(*ca, *cb).map(Type::Object)
            }
            _ => None,
        }
    }
}
