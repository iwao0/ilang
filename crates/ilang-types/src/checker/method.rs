//! Extracted from `checker/mod.rs`.

#![allow(unused_imports)]

use std::collections::{HashMap, HashSet};

use ilang_ast::{
    Block, ClassDecl, CtorArgs, EnumDecl, Expr, ExprKind, FieldDecl, FnDecl, Item, Param,
    PatternBindings, PatternKind, Program, Span, Stmt, StmtKind, Symbol, Type, UnOp,
    VariantPayload,
};

use crate::error::TypeError;
use crate::ops::{assignable, bin_result, int_literal_fits};

use super::*;

impl TypeChecker {
    #[allow(clippy::too_many_arguments)]
    /// Resolve which method overload (or init) a call site invokes.
    /// Returns the chosen Signature; records the pick in the side
    /// table so the post-typecheck mangler can rewrite the call.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn resolve_method_call(
        &self,
        class_name: Symbol,
        method: Symbol,
        sigs: &[Signature],
        args: &[Expr],
        env: &Vars,
        ret_ty: Option<&Type>,
        in_class: Option<Symbol>,
        loop_depth: u32,
        span: Span,
    ) -> Result<Signature, TypeError> {
        // Single overload: keep the precise check_args path so error
        // variants (Mismatch / ArityMismatch) match what users expect.
        if sigs.len() == 1 {
            self.method_overload_pick
                .borrow_mut()
                .insert(span, (class_name.into(), method.into(), 0));
            // Method-level generics (e.g. `Promise.then<U>(cb: fn(T): U)`,
            // `Promise.resolve<T>(v: T)`): infer the per-call type-arg
            // bindings from (parametric param type, arg type) pairs,
            // substitute, then validate. Mirrors the free-fn generic
            // path in `check_call_named`.
            if !sigs[0].type_params.is_empty() {
                let sig = &sigs[0];
                if sig.params.len() != args.len() {
                    return Err(TypeError::ArityMismatch {
                        name: method.into(),
                        expected: sig.params.len(),
                        got: args.len(),
                        span,
                    });
                }
                let mut bindings: HashMap<Symbol, Type> = HashMap::new();
                let mut arg_tys: Vec<Type> = Vec::with_capacity(args.len());
                for (param_ty, arg) in sig.params.iter().zip(args.iter()) {
                    let at =
                        self.check_expr(arg, env, ret_ty, in_class, loop_depth)?;
                    collect_type_var_bindings(param_ty, &at, &mut bindings);
                    arg_tys.push(at);
                }
                let inferred_args: Vec<Type> = sig
                    .type_params
                    .iter()
                    .map(|p| bindings.get(p).cloned().unwrap_or(Type::Any))
                    .collect();
                // Stash the inferred type args so the AST
                // monomorphizer (`monomorphize_methods`) can specialize
                // the method body and rewrite this call's method
                // symbol to the mangled name.
                self.method_call_type_args
                    .borrow_mut()
                    .insert(span, (class_name, method, inferred_args.clone()));
                for ((param_ty, arg), at) in
                    sig.params.iter().zip(args.iter()).zip(arg_tys.iter())
                {
                    let actual = subst_type(param_ty, &sig.type_params, &inferred_args);
                    // Refine an enum-ctor argument from the (substituted)
                    // param type, like the generic-fn and `check_args`
                    // paths. Without this a `Result.ok(..)` passed to a
                    // generic method — e.g. the async desugar's
                    // `Promise.settleResolve(p, Result.ok(..))` against
                    // `v: T = Result<Box,string>` — keeps its unfilled
                    // param `Any` and the monomorphizer chokes.
                    self.refine_enum_ctor_args(arg, &actual);
                    if !self.value_assignable(arg, at, &actual) {
                        return Err(TypeError::Mismatch {
                            expected: actual,
                            got: at.clone(),
                            span: arg.span,
                        });
                    }
                }
                let mut chosen = sig.clone();
                chosen.ret = subst_type(&sig.ret, &sig.type_params, &inferred_args);
                chosen.params = sig
                    .params
                    .iter()
                    .map(|p| subst_type(p, &sig.type_params, &inferred_args))
                    .collect();
                chosen.type_params = Vec::new();
                self.warn_if_deprecated(class_name, method, &chosen, span);
                return Ok(chosen);
            }
            self.check_args(method, &sigs[0], args, env, ret_ty, in_class, loop_depth, span)?;
            self.warn_if_deprecated(class_name, method, &sigs[0], span);
            return Ok(sigs[0].clone());
        }
        // Multiple overloads: score and pick.
        let mut arg_tys: Vec<Type> = Vec::with_capacity(args.len());
        for a in args {
            arg_tys.push(self.check_expr(a, env, ret_ty, in_class, loop_depth)?);
        }
        let chosen = resolve_overload(
            method,
            sigs,
            &arg_tys,
            args,
            span,
            &|c, p| self.subclass_distance(c, p),
        )?;
        let cs = sigs[chosen].clone();
        self.method_overload_pick
            .borrow_mut()
            .insert(span, (class_name.into(), method.into(), chosen));
        self.check_args(method, &cs, args, env, ret_ty, in_class, loop_depth, span)?;
        self.warn_if_deprecated(class_name, method, &cs, span);
        Ok(cs)
    }

    /// Emit a `@deprecated` call-site warning when the chosen
    /// signature carries one. Stores into
    /// `TypeChecker::type_warnings`, which the CLI prints to
    /// stderr after a successful check and the LSP surfaces as a
    /// `DiagnosticSeverity::WARNING`.
    fn warn_if_deprecated(
        &self,
        class_name: Symbol,
        method: Symbol,
        sig: &Signature,
        span: Span,
    ) {
        let Some(reason) = sig.deprecated.as_deref() else {
            return;
        };
        let suffix = if reason.is_empty() {
            String::new()
        } else {
            format!(": {reason}")
        };
        self.warn(
            span,
            format!("`{}.{}` is deprecated{}", class_name, method, suffix),
        );
    }

    pub(super) fn check_args(
        &self,
        name: Symbol,
        sig: &Signature,
        args: &[Expr],
        env: &Vars,
        ret_ty: Option<&Type>,
        in_class: Option<Symbol>,
        loop_depth: u32,
        call_span: Span,
    ) -> Result<(), TypeError> {
        if sig.variadic {
            // Variadic: the declared params are the **fixed prefix**;
            // each arg before that index is type-checked against the
            // declared type (so e.g. `printf`'s `fmt: string` is
            // enforced). Extra args after the prefix are checked
            // permissively — they flow through to the C side at
            // their actual JIT-time types.
            if args.len() < sig.params.len() {
                return Err(TypeError::ArityMismatch {
                    name: name.into(),
                    expected: sig.params.len(),
                    got: args.len(),
                    span: call_span,
                });
            }
            for (i, arg) in args.iter().enumerate() {
                let at = self.or_record(
                    self.check_expr(arg, env, ret_ty, in_class, loop_depth),
                );
                if i < sig.params.len() {
                    let p = &sig.params[i];
                    if !matches!(p, Type::Any) && !self.value_assignable(arg, &at, p) {
                        self.record(TypeError::Mismatch {
                            expected: p.clone(),
                            got: at,
                            span: arg.span,
                        });
                    }
                }
            }
            return Ok(());
        }
        // Default-arg fill: when args are short, try to append the
        // trailing defaults stored on the signature. If any required
        // (no-default) trailing slot is missing, fall through to the
        // normal arity-mismatch error below.
        let filled: Vec<Expr> = if args.len() < sig.params.len() {
            let missing = sig.params.len() - args.len();
            let mut appended: Vec<Expr> = Vec::with_capacity(missing);
            let mut ok = true;
            for d in sig.defaults.iter().skip(args.len()).take(missing) {
                match d {
                    Some(e) => appended.push(e.clone()),
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok {
                self.call_default_fills
                    .borrow_mut()
                    .insert(call_span, appended.clone());
                args.iter().cloned().chain(appended.into_iter()).collect()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        let effective: &[Expr] = if filled.is_empty() { args } else { &filled };
        if sig.params.len() != effective.len() {
            return Err(TypeError::ArityMismatch {
                name: name.into(),
                expected: sig.params.len(),
                got: args.len(),
                span: call_span,
            });
        }
        for (param_ty, arg) in sig.params.iter().zip(effective.iter()) {
            let mut at = self.or_record(
                self.check_expr(arg, env, ret_ty, in_class, loop_depth),
            );
            // Refine an enum-ctor argument from the param's declared type
            // (`f(Result.err("e"))` against `Result<i64,string>`) so the
            // unfilled type param doesn't reach the monomorphizer as Any.
            self.refine_enum_ctor_args(arg, param_ty);
            // Solve a generic fn call argument's return-only type param
            // from the param type (`take(makeArr())` against `i64[]`).
            if let Some(corrected) = self.refine_fn_call_type_args(arg, param_ty) {
                at = corrected;
            }
            if !self.value_assignable(arg, &at, param_ty) {
                self.record(TypeError::Mismatch {
                    expected: param_ty.clone(),
                    got: at,
                    span: arg.span,
                });
            }
        }
        Ok(())
    }

}
