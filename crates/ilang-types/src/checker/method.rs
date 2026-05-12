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
            self.check_args(method, &sigs[0], args, env, ret_ty, in_class, loop_depth, span)?;
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
        Ok(cs)
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
                let at = self.check_expr(arg, env, ret_ty, in_class, loop_depth)?;
                if i < sig.params.len() {
                    let p = &sig.params[i];
                    if !matches!(p, Type::Any) && !self.value_assignable(arg, &at, p) {
                        return Err(TypeError::Mismatch {
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
            let at = self.check_expr(arg, env, ret_ty, in_class, loop_depth)?;
            if !self.value_assignable(arg, &at, param_ty) {
                return Err(TypeError::Mismatch {
                    expected: param_ty.clone(),
                    got: at,
                    span: arg.span,
                });
            }
        }
        Ok(())
    }

}
