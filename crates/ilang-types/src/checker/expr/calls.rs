//! `ExprKind::Call` / `ExprKind::MethodCall` — the two big call
//! shapes of the type checker, extracted from `expr/mod.rs` so
//! the dispatch in `check_expr_inner` stays scannable. Helpers
//! here are called exactly once from the corresponding arm.

#![allow(unused_imports)]

use std::collections::{HashMap, HashSet};

use ilang_ast::{
    Block, ClassDecl, CtorArgs, EnumDecl, Expr, ExprKind, FieldDecl, FnDecl, Item, Param,
    PatternBindings, PatternKind, Program, Span, Stmt, StmtKind, Symbol, Type, UnOp,
    VariantPayload,
};

use crate::error::TypeError;
use crate::ops::{assignable, bin_result, int_literal_fits};

use super::super::utils::check_arity;
use super::super::*;

impl TypeChecker {
    pub(super) fn check_method_call(
        &self,
        obj: &Expr,
        method: &Symbol,
        args: &[Expr],
        env: &Vars,
        ret_ty: Option<&Type>,
        in_class: Option<Symbol>,
        loop_depth: u32,
        span: Span,
    ) -> Result<Type, TypeError> {
        if method == "deinit" {
            return Err(TypeError::CannotCallDeinit { span });
        }
        // Static method dispatch: `ClassName.method(args)` —
        // the receiver is a Var matching a known class name
        // that has a static method by that name, and there's
        // no shadowing local of the same name.
        if let ExprKind::Var(name) = &obj.kind {
            let is_local_shadow = env.contains_key(name) || self.vars.contains_key(name);
            if !is_local_shadow {
                // Built-in `string.*` static factories. Match the
                // primitive-type receiver before the class lookup
                // since `string` isn't registered as a class —
                // without this the receiver would fall through to
                // `check_expr` and error as "undefined variable".
                if name.as_str() == "string" {
                    match method.as_str() {
                        "fromUtf16" => {
                            check_arity(args.len(), 1, method.clone(), span)?;
                            let at = self.check_expr(
                                &args[0], env, ret_ty, in_class, loop_depth,
                            )?;
                            let ok = matches!(
                                &at,
                                Type::Array { elem, .. } if matches!(**elem, Type::U16),
                            );
                            if !ok {
                                return Err(TypeError::Mismatch {
                                    expected: Type::Array {
                                        elem: Box::new(Type::U16),
                                        fixed: None,
                                    },
                                    got: at,
                                    span: args[0].span,
                                });
                            }
                            return Ok(Type::Str);
                        }
                        _ => {
                            return Err(TypeError::UnknownMethod {
                                class: "string".into(),
                                method: method.clone(),
                                span,
                            });
                        }
                    }
                }
                // Walk the parent chain so an inherited static
                // (`SKScene.alloc()`, where `alloc` lives on
                // SKNode → NSObject) resolves through the
                // subclass's name. Without the climb, the
                // statics-on-this-class lookup misses inherited
                // factories and the receiver falls through to
                // `check_expr`, which errors as
                // "undefined variable" on the class-name `Var`.
                let mut cur = Some(*name);
                while let Some(cur_name) = cur {
                    let Some(cls) = self.classes.get(&cur_name) else { break };
                    if let Some(sigs) = cls.static_methods.get(method).cloned() {
                        let cmod = cls.module.clone();
                        let cn = cur_name.as_str().to_string();
                        // Visibility: if *any* overload is pub the
                        // name is reachable cross-module; the
                        // overload resolver then picks the one
                        // that matches argument types.
                        let any_pub = sigs.iter().any(|s| s.is_pub);
                        self.require_visible(
                            &cn, &cmod, "static method", method.as_str(), any_pub, span,
                        )?;
                        let chosen = self.resolve_method_call(
                            *name, *method, &sigs, args, env, ret_ty, in_class,
                            loop_depth, span, &[],
                        )?;
                        return Ok(chosen.ret);
                    }
                    cur = cls.parent;
                }
            }
        }
        let ot = self.check_expr(obj, env, ret_ty, in_class, loop_depth)?;
        // Built-in `Type` introspection methods.
        if matches!(&ot, Type::Object(n) if n.as_str() == "Type") {
            let target = match method.as_str() {
                "fieldType" | "methodReturn" => Some((1, Type::Optional(Box::new(
                    Type::Object("Type".into()),
                )))),
                "methodParams" => Some((1, Type::Optional(Box::new(Type::Array {
                    elem: Box::new(Type::Object("Type".into())),
                    fixed: None,
                })))),
                _ => None,
            };
            if let Some((arity, ret)) = target {
                check_arity(args.len(), arity, method.clone(), span)?;
                let at = self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
                if at != Type::Str {
                    return Err(TypeError::Mismatch {
                        expected: Type::Str,
                        got: at,
                        span: args[0].span,
                    });
                }
                return Ok(ret);
            }
        }
        // Built-in Weak method: get(): T?.
        if let Type::Weak(inner) = &ot {
            if method == "get" {
                check_arity(args.len(), 0, "get".into(), span)?;
                return Ok(Type::Optional(inner.clone()));
            }
            return Err(TypeError::UnknownMethod {
                class: Symbol::intern(&format!("{ot}")),
                method: method.clone(),
                span,
            });
        }
        // Built-in Optional methods: unwrap. (`isSome` / `isNone`
        // are properties — see ExprKind::Field.)
        if let Type::Optional(inner) = &ot {
            match method.as_str() {
                "unwrap" => {
                    check_arity(args.len(), 0, method.clone(), span)?;
                    return Ok((**inner).clone());
                }
                _ => {
                    return Err(TypeError::UnknownMethod {
                        class: Symbol::intern(&format!("{ot}")),
                        method: method.clone(),
                        span,
                    });
                }
            }
        }
        // Built-in `ObjCBlock<fn(...)>.invoke(args)` — call the
        // ObjC block represented by `ot`. The block's underlying
        // fn type lives in `args[0]` of the Generic, so the
        // method's argument list / return type are derived from
        // it rather than from a static signature table.
        if let Type::Generic(g) = &ot {
            if g.base.as_str() == "ObjCBlock" && g.args.len() == 1 {
                if let Type::Fn(ft) = &g.args[0] {
                    if method.as_str() == "invoke" {
                        check_arity(args.len(), ft.params.len(), method.clone(), span)?;
                        for (i, expected) in ft.params.iter().enumerate() {
                            let got = self.check_expr(
                                &args[i], env, ret_ty, in_class, loop_depth,
                            )?;
                            if &got != expected {
                                return Err(TypeError::Mismatch {
                                    expected: expected.clone(),
                                    got,
                                    span: args[i].span,
                                });
                            }
                        }
                        // i64-returning blocks need to dispatch to
                        // the obj-to-obj runtime invoker; mark the
                        // span so the mangler can rewrite the
                        // method name (MIR doesn't see the
                        // ObjCBlock<fn(...): R> type since the
                        // receiver lowers to plain MirTy::I64).
                        // Only the `(i64) -> i64` shape is
                        // currently bound in the runtime; other
                        // i64-returning shapes still error out at
                        // MIR.
                        if ft.ret != Type::Unit {
                            self.objc_invoke_obj_to_obj_spans
                                .borrow_mut()
                                .insert(span);
                        }
                        return Ok(ft.ret.clone());
                    }
                    return Err(TypeError::UnknownMethod {
                        class: Symbol::intern(&format!("{ot}")),
                        method: method.clone(),
                        span,
                    });
                }
            }
        }
        // Built-in `.toString()` for numeric primitives and
        // `bool`. Decimal for ints, JS-style for floats
        // (matching `console.log`'s formatting), `"true"` /
        // `"false"` for bool.
        if (ot.is_numeric() || ot == Type::Bool) && method.as_str() == "toString" {
            check_arity(args.len(), 0, method.clone(), span)?;
            return Ok(Type::Str);
        }
        // `.isFinite()` / `.isNaN()` on f32 / f64. Integers can't
        // be NaN / infinite by construction, so these methods are
        // gated to floats only.
        if matches!(ot, Type::F32 | Type::F64)
            && matches!(method.as_str(), "isFinite" | "isNaN")
        {
            check_arity(args.len(), 0, method.clone(), span)?;
            return Ok(Type::Bool);
        }
        // `.hashCode(): i64` on every numeric primitive and `bool`.
        // The same protocol `Set<MyClass>` / `Map<MyClass, V>` use
        // — having it on every primitive lets `@derive(Hash)`
        // route each field through `field.hashCode()` regardless
        // of width. Ints widen, bool becomes 0 / 1, floats are
        // bit-cast (so distinct NaN payloads keep distinct
        // hashes, matching how Set<f64> already keys them).
        if (ot.is_numeric() || ot == Type::Bool) && method.as_str() == "hashCode" {
            check_arity(args.len(), 0, method.clone(), span)?;
            return Ok(Type::I64);
        }
        // Primitive numeric / bool receivers reach here only when
        // none of the small built-in method set above matched
        // (`toString`, and for floats `isFinite` / `isNaN`). Emit
        // a targeted `UnknownMethod` error against the receiver
        // type — otherwise the receiver falls through to
        // `expect_object` and surfaces a useless "expected
        // <object>, got f64" message for what is almost always a
        // typo (e.g. `isNan` for `isNaN`).
        if ot.is_numeric() || ot == Type::Bool {
            return Err(TypeError::UnknownMethod {
                class: Symbol::intern(&format!("{ot}")),
                method: method.clone(),
                span,
            });
        }
        // Built-in string methods (JS-style camelCase).
        if matches!(ot, Type::Str) {
            let arity_check = |expected: usize| -> Result<(), TypeError> {
                if args.len() != expected {
                    Err(TypeError::ArityMismatch {
                        name: method.clone(),
                        expected,
                        got: args.len(),
                        span,
                    })
                } else {
                    Ok(())
                }
            };
            match method.as_str() {
                "charAt" => {
                    arity_check(1)?;
                    let at = self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
                    if !matches!(at, Type::I64 | Type::I32 | Type::I16 | Type::I8 | Type::U64 | Type::U32 | Type::U16 | Type::U8) {
                        return Err(TypeError::Mismatch {
                            expected: Type::I64,
                            got: at,
                            span: args[0].span,
                        });
                    }
                    return Ok(Type::Str);
                }
                "includes" | "startsWith" | "endsWith" => {
                    arity_check(1)?;
                    let at = self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
                    if !matches!(at, Type::Str) {
                        return Err(TypeError::Mismatch {
                            expected: Type::Str,
                            got: at,
                            span: args[0].span,
                        });
                    }
                    return Ok(Type::Bool);
                }
                "toUpper" | "toLower" | "trim" => {
                    arity_check(0)?;
                    return Ok(Type::Str);
                }
                "replace" => {
                    arity_check(2)?;
                    for a in args {
                        let at = self.check_expr(a, env, ret_ty, in_class, loop_depth)?;
                        if !matches!(at, Type::Str) {
                            return Err(TypeError::Mismatch {
                                expected: Type::Str,
                                got: at,
                                span: a.span,
                            });
                        }
                    }
                    return Ok(Type::Str);
                }
                "split" => {
                    arity_check(1)?;
                    let at = self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
                    if !matches!(at, Type::Str) {
                        return Err(TypeError::Mismatch {
                            expected: Type::Str,
                            got: at,
                            span: args[0].span,
                        });
                    }
                    return Ok(Type::Array { elem: Box::new(Type::Str), fixed: None });
                }
                "slice" => {
                    arity_check(2)?;
                    for a in args {
                        let at = self.check_expr(a, env, ret_ty, in_class, loop_depth)?;
                        if !matches!(at, Type::I64 | Type::I32 | Type::I16 | Type::I8 | Type::U64 | Type::U32 | Type::U16 | Type::U8) {
                            return Err(TypeError::Mismatch {
                                expected: Type::I64,
                                got: at,
                                span: a.span,
                            });
                        }
                    }
                    return Ok(Type::Str);
                }
                "concat" => {
                    arity_check(1)?;
                    let at = self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
                    if !matches!(at, Type::Str) {
                        return Err(TypeError::Mismatch {
                            expected: Type::Str,
                            got: at,
                            span: args[0].span,
                        });
                    }
                    return Ok(Type::Str);
                }
                "indexOf" | "lastIndexOf" => {
                    if args.is_empty() || args.len() > 2 {
                        return Err(TypeError::ArityMismatch {
                            name: method.clone(),
                            expected: 1,
                            got: args.len(),
                            span,
                        });
                    }
                    let needle_ty =
                        self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
                    if !matches!(needle_ty, Type::Str) {
                        return Err(TypeError::Mismatch {
                            expected: Type::Str,
                            got: needle_ty,
                            span: args[0].span,
                        });
                    }
                    if let Some(from) = args.get(1) {
                        let from_ty =
                            self.check_expr(from, env, ret_ty, in_class, loop_depth)?;
                        if !matches!(
                            from_ty,
                            Type::I64
                                | Type::I32
                                | Type::I16
                                | Type::I8
                                | Type::U64
                                | Type::U32
                                | Type::U16
                                | Type::U8
                        ) {
                            return Err(TypeError::Mismatch {
                                expected: Type::I64,
                                got: from_ty,
                                span: from.span,
                            });
                        }
                    }
                    return Ok(Type::I64);
                }
                "encodeUtf16" => {
                    // 0 or 1 args. The optional 1st arg is the
                    // NUL-terminator flag and must be a bool. The
                    // default (when omitted) is `true`, matching
                    // Win32 W-suffix API expectations — see the
                    // MIR lowering for the actual padding.
                    if args.len() > 1 {
                        return Err(TypeError::ArityMismatch {
                            name: method.clone(),
                            expected: 0,
                            got: args.len(),
                            span,
                        });
                    }
                    if let Some(flag) = args.first() {
                        let ft = self.check_expr(flag, env, ret_ty, in_class, loop_depth)?;
                        if !matches!(ft, Type::Bool) {
                            return Err(TypeError::Mismatch {
                                expected: Type::Bool,
                                got: ft,
                                span: flag.span,
                            });
                        }
                    }
                    return Ok(Type::Array {
                        elem: Box::new(Type::U16),
                        fixed: None,
                    });
                }
                "hashCode" => {
                    // Zero-arg, returns i64. Stable FNV-1a hash over
                    // the string's bytes — the runtime call lives in
                    // `crates/ilang-runtime/src/strings.rs`. Lets
                    // `@derive(Hash)` recurse through string fields
                    // and lets user code key its own data structures
                    // off `"foo".hashCode()`.
                    if !args.is_empty() {
                        return Err(TypeError::ArityMismatch {
                            name: method.clone(),
                            expected: 0,
                            got: args.len(),
                            span,
                        });
                    }
                    return Ok(Type::I64);
                }
                _ => {
                    return Err(TypeError::UnknownMethod {
                        class: "string".into(),
                        method: method.clone(),
                        span,
                    });
                }
            }
        }
        // Built-in array methods.
        if let Type::Array { elem, fixed } = &ot {
            if method == "push" {
                if fixed.is_some() {
                    return Err(TypeError::Mismatch {
                        expected: Type::Array {
                            elem: elem.clone(),
                            fixed: None,
                        },
                        got: ot.clone(),
                        span,
                    });
                }
                check_arity(args.len(), 1, "push".into(), span)?;
                let at = self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
                if !self.value_assignable(&args[0], &at, elem) {
                    return Err(TypeError::Mismatch {
                        expected: (**elem).clone(),
                        got: at,
                        span: args[0].span,
                    });
                }
                // Refine an enum ctor arg (e.g. `xs.push(Result.err("e"))`)
                // from the element type so the monomorphizer sees a
                // concrete instantiation instead of `Any`.
                self.refine_enum_ctor_args(&args[0], elem);
                return Ok(Type::Unit);
            }
            if method == "pop" {
                if fixed.is_some() {
                    return Err(TypeError::Mismatch {
                        expected: Type::Array {
                            elem: elem.clone(),
                            fixed: None,
                        },
                        got: ot.clone(),
                        span,
                    });
                }
                check_arity(args.len(), 0, "pop".into(), span)?;
                return Ok(Type::Optional(elem.clone()));
            }
            if method == "removeAt" {
                if fixed.is_some() {
                    return Err(TypeError::Mismatch {
                        expected: Type::Array {
                            elem: elem.clone(),
                            fixed: None,
                        },
                        got: ot.clone(),
                        span,
                    });
                }
                check_arity(args.len(), 1, "removeAt".into(), span)?;
                let at = self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
                if !self.value_assignable(&args[0], &at, &Type::I64) {
                    return Err(TypeError::Mismatch {
                        expected: Type::I64,
                        got: at,
                        span: args[0].span,
                    });
                }
                return Ok(Type::Optional(elem.clone()));
            }
            if method == "remove" {
                if fixed.is_some() {
                    return Err(TypeError::Mismatch {
                        expected: Type::Array {
                            elem: elem.clone(),
                            fixed: None,
                        },
                        got: ot.clone(),
                        span,
                    });
                }
                check_arity(args.len(), 1, "remove".into(), span)?;
                let at = self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
                if !self.value_assignable(&args[0], &at, elem) {
                    return Err(TypeError::Mismatch {
                        expected: (**elem).clone(),
                        got: at,
                        span: args[0].span,
                    });
                }
                self.refine_enum_ctor_args(&args[0], elem);
                return Ok(Type::Bool);
            }
            if method == "indexOf" || method == "includes" {
                check_arity(args.len(), 1, method.clone(), span)?;
                let at = self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
                if !self.value_assignable(&args[0], &at, elem) {
                    return Err(TypeError::Mismatch {
                        expected: (**elem).clone(),
                        got: at,
                        span: args[0].span,
                    });
                }
                self.refine_enum_ctor_args(&args[0], elem);
                return Ok(if method == "indexOf" {
                    Type::I64
                } else {
                    Type::Bool
                });
            }
            if method == "slice" {
                // slice(start: i64, end: i64): T[]
                check_arity(args.len(), 2, "slice".into(), span)?;
                for a in args {
                    let at = self.check_expr(a, env, ret_ty, in_class, loop_depth)?;
                    if !self.value_assignable(a, &at, &Type::I64) {
                        return Err(TypeError::Mismatch {
                            expected: Type::I64,
                            got: at,
                            span: a.span,
                        });
                    }
                }
                return Ok(Type::Array { elem: elem.clone(), fixed: None });
            }
            if matches!(
                method.as_str(),
                "map" | "filter" | "forEach" | "find" | "findIndex" | "every" | "some",
            ) {
                check_arity(args.len(), 1, method.clone(), span)?;
                let ft = self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
                let (params, ret) = match &ft {
                    Type::Fn(fty) => (fty.params.clone(), fty.ret.clone()),
                    _ => return Err(TypeError::Mismatch {
                        expected: Type::func(vec![(**elem).clone()], Type::Any),
                        got: ft,
                        span: args[0].span,
                    }),
                };
                if params.len() != 1 || !assignable(elem, &params[0]) && !self.assignable_obj(elem, &params[0]) {
                    return Err(TypeError::Mismatch {
                        expected: Type::func(vec![(**elem).clone()], Type::Any),
                        got: Type::func(params.to_vec(), ret.clone()),
                        span: args[0].span,
                    });
                }
                // `filter` / `find` / `findIndex` / `every` / `some`
                // all take a `fn(T): bool` predicate; reject mismatched
                // returns with a single shared check.
                let needs_bool = matches!(
                    method.as_str(),
                    "filter" | "find" | "findIndex" | "every" | "some",
                );
                if needs_bool && !matches!(ret, Type::Bool) {
                    return Err(TypeError::Mismatch {
                        expected: Type::Bool,
                        got: ret,
                        span: args[0].span,
                    });
                }
                return Ok(match method.as_str() {
                    "map" => Type::Array { elem: Box::new(ret), fixed: None },
                    "filter" => Type::Array { elem: elem.clone(), fixed: None },
                    "forEach" => Type::Unit,
                    "find" => Type::Optional(elem.clone()),
                    "findIndex" => Type::I64,
                    "every" | "some" => Type::Bool,
                    _ => unreachable!(),
                });
            }
            if method == "concat" {
                check_arity(args.len(), 1, "concat".into(), span)?;
                let at = self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
                let other_elem = match &at {
                    Type::Array { elem: e, .. } => (**e).clone(),
                    _ => return Err(TypeError::Mismatch {
                        expected: Type::Array {
                            elem: elem.clone(),
                            fixed: None,
                        },
                        got: at,
                        span: args[0].span,
                    }),
                };
                if !assignable(elem, &other_elem)
                    && !self.assignable_obj(elem, &other_elem)
                {
                    return Err(TypeError::Mismatch {
                        expected: Type::Array {
                            elem: elem.clone(),
                            fixed: None,
                        },
                        got: at,
                        span: args[0].span,
                    });
                }
                return Ok(Type::Array { elem: elem.clone(), fixed: None });
            }
            if method == "reverse" {
                check_arity(args.len(), 0, "reverse".into(), span)?;
                return Ok(Type::Array { elem: elem.clone(), fixed: None });
            }
            if method == "join" {
                // Only `string[]` has a natural `join`. For numeric
                // arrays the user can `.map(x -> x.toString())` first.
                if !matches!(**elem, Type::Str) {
                    return Err(TypeError::UnknownMethod {
                        class: Symbol::intern(&format!("{ot}")),
                        method: method.clone(),
                        span,
                    });
                }
                check_arity(args.len(), 1, "join".into(), span)?;
                let at = self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
                if !matches!(at, Type::Str) {
                    return Err(TypeError::Mismatch {
                        expected: Type::Str,
                        got: at,
                        span: args[0].span,
                    });
                }
                return Ok(Type::Str);
            }
            if method == "shift" {
                if fixed.is_some() {
                    return Err(TypeError::Mismatch {
                        expected: Type::Array {
                            elem: elem.clone(),
                            fixed: None,
                        },
                        got: ot.clone(),
                        span,
                    });
                }
                check_arity(args.len(), 0, "shift".into(), span)?;
                return Ok(Type::Optional(elem.clone()));
            }
            if method == "unshift" {
                if fixed.is_some() {
                    return Err(TypeError::Mismatch {
                        expected: Type::Array {
                            elem: elem.clone(),
                            fixed: None,
                        },
                        got: ot.clone(),
                        span,
                    });
                }
                check_arity(args.len(), 1, "unshift".into(), span)?;
                let at = self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
                if !self.value_assignable(&args[0], &at, elem) {
                    return Err(TypeError::Mismatch {
                        expected: (**elem).clone(),
                        got: at,
                        span: args[0].span,
                    });
                }
                self.refine_enum_ctor_args(&args[0], elem);
                return Ok(Type::Unit);
            }
            if method == "fill" {
                check_arity(args.len(), 1, "fill".into(), span)?;
                let at = self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
                if !self.value_assignable(&args[0], &at, elem) {
                    return Err(TypeError::Mismatch {
                        expected: (**elem).clone(),
                        got: at,
                        span: args[0].span,
                    });
                }
                self.refine_enum_ctor_args(&args[0], elem);
                return Ok(Type::Unit);
            }
            if method == "sort" {
                check_arity(args.len(), 1, "sort".into(), span)?;
                let ft = self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
                let (params, ret) = match &ft {
                    Type::Fn(fty) => (fty.params.clone(), fty.ret.clone()),
                    _ => return Err(TypeError::Mismatch {
                        expected: Type::func(
                            vec![(**elem).clone(), (**elem).clone()],
                            Type::I64,
                        ),
                        got: ft,
                        span: args[0].span,
                    }),
                };
                let two_args_ok = params.len() == 2
                    && (assignable(elem, &params[0])
                        || self.assignable_obj(elem, &params[0]))
                    && (assignable(elem, &params[1])
                        || self.assignable_obj(elem, &params[1]));
                if !two_args_ok || !matches!(ret, Type::I64) {
                    return Err(TypeError::Mismatch {
                        expected: Type::func(
                            vec![(**elem).clone(), (**elem).clone()],
                            Type::I64,
                        ),
                        got: Type::func(params.to_vec(), ret.clone()),
                        span: args[0].span,
                    });
                }
                return Ok(Type::Array { elem: elem.clone(), fixed: None });
            }
            return Err(TypeError::UnknownMethod {
                class: Symbol::intern(&format!("{ot}")),
                method: method.clone(),
                span,
            });
        }
        // `@flags` enum: `f.has(other)` is a synthetic method
        // returning bool, equivalent to `(f & other) == other`.
        if let Type::Object(ename) = &ot {
            if let Some(sig) = self.enums.get(ename).cloned() {
                if sig.flags && method == "has" {
                    check_arity(args.len(), 1, "has".into(), span)?;
                    let at = self.check_expr(
                        &args[0], env, ret_ty, in_class, loop_depth,
                    )?;
                    if at != ot {
                        return Err(TypeError::Mismatch {
                            expected: ot.clone(),
                            got: at,
                            span: args[0].span,
                        });
                    }
                    return Ok(Type::Bool);
                }
            }
        }
        // `*T.Method(args)` where T is a CRepr struct and Method
        // is a fn-typed field — COM vtable dispatch. The field
        // holds a raw fn pointer; the call shape is determined by
        // the fn type written in the struct declaration.
        if let Type::RawPtr { inner, .. } = &ot {
            if let Type::Object(struct_name) = &**inner {
                if let Some(cls) = self.classes.get(struct_name) {
                    if cls.is_repr_c {
                        if let Some(field_ty) = cls.fields.get(method).cloned() {
                            if let Type::Fn(ft) = field_ty {
                                return self.check_fn_field_call(
                                    &ft, args, env, ret_ty, in_class, loop_depth,
                                    method.clone(), span,
                                );
                            }
                            return Err(TypeError::UnknownMethod {
                                class: (*struct_name).into(),
                                method: method.clone(),
                                span,
                            });
                        }
                        return Err(TypeError::UnknownMethod {
                            class: (*struct_name).into(),
                            method: method.clone(),
                            span,
                        });
                    }
                }
            }
        }
        let class_name = expect_object(&ot, span)?;
        // Receiver typed as an interface: look the method up
        // on the interface itself; runtime resolves the
        // implementing fn from the receiver's actual class.
        if let Some(isig) = self.interfaces.get(&class_name).cloned() {
            // `@com interface` parents chain through `: Parent` so a
            // method declared on the IUnknown root resolves via the
            // leaf interface name. Plain interfaces have no parent
            // today and exit the loop on the first miss.
            //
            // `check.rs` validates that the parent chain is acyclic
            // and that every named parent resolves, but be defensive
            // anyway — the type checker doesn't short-circuit on
            // errors, so an interface that failed validation could
            // still reach here with a stale `parent` in some
            // execution paths. The `visited` set keeps the walk
            // bounded regardless.
            let mut found: Option<InterfaceMethodSig> = None;
            let mut visited: std::collections::HashSet<Symbol> =
                std::collections::HashSet::new();
            visited.insert(class_name);
            let mut cur = Some(isig.clone());
            while let Some(s) = cur {
                if let Some(im) = s.methods.iter().find(|m| m.name == *method).cloned() {
                    found = Some(im);
                    break;
                }
                cur = s.parent.as_ref().and_then(|p| {
                    if !visited.insert(*p) {
                        return None;
                    }
                    self.interfaces.get(p).cloned()
                });
            }
            if let Some(im) = found {
                let sig = Signature {
                    params: im.params.clone(),
                    ret: im.ret.clone(),
                    variadic: false,
                    type_params: Vec::new(),
                    decl_span: span,
                    defaults: vec![None; im.params.len()],
                    is_pub: true,
            deprecated: None,
                    lib_names: Vec::new(),
                };
                let chosen = self.resolve_method_call(
                    class_name.into(), *method, &[sig], args, env, ret_ty, in_class,
                    loop_depth, span, &[],
                )?;
                return Ok(chosen.ret);
            }
            return Err(TypeError::UnknownMethod {
                class: class_name.into(),
                method: method.clone(),
                span,
            });
        }
        let cls = self.classes.get(&class_name).ok_or_else(|| {
            TypeError::UndefinedClass {
                name: class_name.into(),
                span,
            }
        })?;
        let raw_sigs = match cls.methods.get(method) {
            Some(s) => s,
            None => {
                // Fn-typed instance field — `obj.field(args)`
                // desugars to a field load + indirect call instead
                // of a method dispatch. Lets users avoid the
                // `let cb = obj.field; cb(args)` bounce when the
                // field already carries the fn type.
                if let Some(field_ty) = cls.fields.get(method).cloned() {
                    if let Type::Fn(ft) = field_ty {
                        return self.check_fn_field_call(
                            &ft, args, env, ret_ty, in_class, loop_depth,
                            method.clone(), span,
                        );
                    }
                }
                return Err(TypeError::UnknownMethod {
                    class: class_name.into(),
                    method: method.clone(),
                    span,
                });
            }
        };
        let class_params = cls.type_params.clone();
        let inst_args: Vec<Type> = type_args_of(&ot).to_vec();
        // Substitute generic type args once per overload, then
        // resolve which overload matches the call.
        let class_name_owned = class_name.into();
        let substituted: Vec<Signature> = raw_sigs
            .iter()
            .map(|raw| Signature {
                params: raw
                    .params
                    .iter()
                    .map(|t| subst_type(t, &class_params, &inst_args))
                    .collect(),
                ret: subst_type(&raw.ret, &class_params, &inst_args),
                variadic: raw.variadic,
                // Keep method-level type_params after class-level
                // substitution. `Promise.then<U>` carries U through
                // even after T was substituted from the receiver's
                // `Promise<T>` class args.
                type_params: raw.type_params.clone(),
                decl_span: raw.decl_span,
                defaults: raw.defaults.clone(),
                is_pub: raw.is_pub,
                deprecated: raw.deprecated.clone(), lib_names: raw.lib_names.clone(),
            })
            .collect();
        // At least one overload must be reachable from the
        // current module for the call to be legal. We check
        // pub on the chosen overload after resolution to
        // surface a precise error.
        let chosen = self.resolve_method_call(
            class_name_owned, *method, &substituted, args, env, ret_ty, in_class, loop_depth, span,
            &inst_args,
        )?;
        let cmod = cls.module.clone();
        self.require_visible(
            class_name.as_str(), &cmod, "method", method.as_str(), chosen.is_pub, span,
        )?;
        Ok(chosen.ret)
    }

    /// `obj.method(args)` where the lookup landed on a fn-typed
    /// instance field instead of a real method — the dispatch
    /// degenerates into a load + indirect call. Both the regular
    /// path (`Object(c).field` of `Type::Fn`) and the CRepr COM
    /// vtable path (`*T.field`) share this check shape.
    fn check_fn_field_call(
        &self,
        ft: &ilang_ast::FnTy,
        args: &[Expr],
        env: &Vars,
        ret_ty: Option<&Type>,
        in_class: Option<Symbol>,
        loop_depth: u32,
        method_name: Symbol,
        span: Span,
    ) -> Result<Type, TypeError> {
        check_arity(args.len(), ft.params.len(), method_name, span)?;
        for (i, expected) in ft.params.iter().enumerate() {
            let got = self.check_expr(&args[i], env, ret_ty, in_class, loop_depth)?;
            self.refine_enum_ctor_args(&args[i], expected);
            if !self.value_assignable(&args[i], &got, expected) {
                return Err(TypeError::Mismatch {
                    expected: expected.clone(),
                    got,
                    span: args[i].span,
                });
            }
        }
        Ok(ft.ret.clone())
    }
}

impl TypeChecker {
    pub(super) fn check_call_expr(
        &self,
        callee: &Symbol, args: &[Expr],
        env: &Vars,
        ret_ty: Option<&Type>,
        in_class: Option<Symbol>,
        loop_depth: u32,
        span: Span,
    ) -> Result<Type, TypeError> {
        if callee == "deinit" {
            return Err(TypeError::CannotCallDeinit { span });
        }
        // Built-in `typeof(x): Type` — RTTI introspection.
        // Accepts any single value; the JIT / interpreter
        // synthesise the right Type metadata at runtime.
        if callee == "typeof" {
            check_arity(args.len(), 1, callee.clone(), span)?;
            self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
            return Ok(Type::Object("Type".into()));
        }
        // `$ffi.cstrFromString(s: string): *const char` — parser-
        // synthesised by the @objc desugar. The `$` prefix is
        // unreachable from user code (lex rejects it), so this only
        // matches compiler-generated calls; no `pub` / module
        // bookkeeping needed.
        if callee.as_str() == "$ffi.cstrFromString" {
            check_arity(args.len(), 1, callee.clone(), span)?;
            self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
            return Ok(Type::RawPtr {
                is_const: true,
                inner: Box::new(Type::CChar),
            });
        }
        // FFI marshalling helpers are only callable inside an
        // `@extern(C) {}` block — they exist to bridge raw C
        // values to ilang-native ones, which only matters at
        // the FFI boundary.
        if !*self.in_extern_c.borrow()
            && FFI_HELPERS.contains(&callee.as_str())
        {
            return Err(TypeError::Unsupported {
                what: format!(
                    "{callee}: FFI marshalling helper, only \
                     callable inside an @extern(C) {{ ... }} block"
                ),
                span,
            });
        }
        // Indirect call through a function-typed local: shadows
        // both methods and top-level fns, mirroring how a local
        // `let print = ...` shadows an outer name.
        if let Some(Type::Fn(ft)) = env.get(callee).cloned() {
            let sig = Signature {
                params: ft.params.to_vec(),
                ret: ft.ret.clone(),
                variadic: false, decl_span: Span::dummy(), type_params: Vec::new(),
                defaults: Vec::new(), is_pub: true, deprecated: None, lib_names: Vec::new() };
            self.check_args(*callee, &sig, args, env, ret_ty, in_class, loop_depth, span)?;
            return Ok(sig.ret);
        }
        if let Some(class_name) = in_class {
            if let Some(cls) = self.classes.get(&class_name) {
                if let Some(sigs) = cls.methods.get(callee) {
                    // Implicit-this method call. Resolve overload
                    // exactly like a top-level fn call.
                    let chosen = self.resolve_method_call(
                        class_name, *callee, sigs, args, env, ret_ty, in_class, loop_depth, span, &[],
                    )?;
                    return Ok(chosen.ret);
                }
            }
        }
        let sigs = self.fns.get(callee).ok_or_else(|| {
            TypeError::UndefinedFunction {
                name: callee.clone(),
                span,
            }
        })?;
        // `@lib(...) pub fn ...` declarations resolve through the
        // extern codegen's dlsym path; calling one from ordinary code
        // compiles but panics at JIT time with `can't resolve symbol
        // X`. Gate the call so the diagnostic lands at the source.
        //
        // ilang-runtime hooks that used to need an exemption now go
        // through `@intrinsic(...)`, which the parser emits with empty
        // `lib_names` — those fall through this check naturally.
        if !*self.in_extern_c.borrow() {
            let all_dlsym = !sigs.is_empty()
                && sigs.iter().all(|s| !s.lib_names.is_empty());
            if all_dlsym {
                let libs_label = sigs[0]
                    .lib_names
                    .iter()
                    .map(|s| format!("\"{}\"", s.as_str()))
                    .collect::<Vec<_>>()
                    .join(", ");
                self.record(TypeError::Unsupported {
                    what: format!(
                        "{callee:?}: @lib({libs_label}) extern declaration, \
                         only callable inside an @extern(...) {{ ... }} block"
                    ),
                    span,
                });
                return Ok(Type::Error);
            }
        }
        // Generic fns can't share a name with overloads (we
        // reject that at registration time), so a generic slot
        // is always exactly one signature. Fall through to the
        // existing generic-inference path below.
        let is_generic_slot = sigs.len() == 1 && !sigs[0].type_params.is_empty();
        if !is_generic_slot {
            if sigs.len() == 1 {
                // Single non-generic overload: keep the existing
                // arity / per-arg validation so error variants
                // (Mismatch / ArityMismatch) stay precise.
                let sig = sigs[0].clone();
                self.fn_overload_pick
                    .borrow_mut()
                    .insert(span, (callee.clone(), 0));
                self.check_args(*callee, &sig, args, env, ret_ty, in_class, loop_depth, span)?;
                return Ok(sig.ret);
            }
            // Multiple overloads — score each viable signature
            // and pick the best match.
            let mut arg_tys: Vec<Type> = Vec::with_capacity(args.len());
            for a in args {
                arg_tys.push(self.check_expr(a, env, ret_ty, in_class, loop_depth)?);
            }
            let chosen = resolve_overload(
                *callee,
                sigs,
                &arg_tys,
                args,
                span,
                &|c, p| self.subclass_distance(c, p),
            )?;
            let chosen_sig = sigs[chosen].clone();
            self.fn_overload_pick
                .borrow_mut()
                .insert(span, (callee.clone(), chosen));
            self.check_args(*callee, &chosen_sig, args, env, ret_ty, in_class, loop_depth, span)?;
            return Ok(chosen_sig.ret);
        }
        let sig = sigs[0].clone();
        // Generic fn — see below; we also stash the inferred
        // type-args vector keyed by call span so the JIT's
        // monomorphization pass can find it later.
        // Generic fn: infer type-arg bindings from the (parametric
        // param type, arg type) pairs, then validate arg-by-arg
        // against the substituted param types and return the
        // substituted return type. Mirrors enum-ctor inference.
        check_arity(args.len(), sig.params.len(), callee.clone(), span)?;
        let mut bindings: HashMap<Symbol, Type> = HashMap::new();
        let mut arg_tys: Vec<Type> = Vec::with_capacity(args.len());
        for (param_ty, arg) in sig.params.iter().zip(args.iter()) {
            let at = self.check_expr(arg, env, ret_ty, in_class, loop_depth)?;
            collect_type_var_bindings(param_ty, &at, &mut bindings);
            arg_tys.push(at);
        }
        let inferred_args: Vec<Type> = sig
            .type_params
            .iter()
            .map(|p| bindings.get(p).cloned().unwrap_or(Type::Any))
            .collect();
        // Stash for the JIT monomorphizer. Args may still
        // contain TypeVars when the call is inside another
        // generic context — that's resolved at expansion time.
        self.fn_call_type_args
            .borrow_mut()
            .insert(span, (callee.clone(), inferred_args.clone()));
        for ((param_ty, arg), at) in sig.params.iter().zip(args.iter()).zip(arg_tys.iter()) {
            let actual = subst_type(param_ty, &sig.type_params, &inferred_args);
            // Refine an enum-ctor argument from the (substituted) param
            // type so `f(Result.err("e"))` against a `Result<i64,string>`
            // param fills the unfilled `T` (else the monomorphizer hits
            // Type::Any). Same fix as the field / array / let positions.
            self.refine_enum_ctor_args(arg, &actual);
            if !self.value_assignable(arg, at, &actual) {
                return Err(TypeError::Mismatch {
                    expected: actual,
                    got: at.clone(),
                    span: arg.span,
                });
            }
        }
        Ok(subst_type(&sig.ret, &sig.type_params, &inferred_args))
    }
}

impl TypeChecker {
    pub(super) fn check_new(
        &self,
        class: &Symbol, type_args: &[Type], args: &[Expr], init_method: &Option<Symbol>,
        env: &Vars,
        ret_ty: Option<&Type>,
        in_class: Option<Symbol>,
        loop_depth: u32,
        span: Span,
    ) -> Result<Type, TypeError> {
        // `new ObjCBlock(closure)` — infer F from the closure's
        // fn type so the user doesn't have to write
        // `new ObjCBlock<fn(i64, i64, i64): ()>(closure)` every time.
        // Require exactly one arg that type-checks as a fn type; the
        // lower pass will further validate that the shape matches one
        // of the runtime's pre-baked invoke trampolines.
        if class.as_str() == "ObjCBlock" && type_args.is_empty() {
            check_arity(args.len(), 1, Symbol::intern("ObjCBlock::init"), span)?;
            let arg_ty = self.check_expr(
                &args[0], env, ret_ty, in_class, loop_depth,
            )?;
            if !matches!(arg_ty, Type::Fn(_)) {
                return Err(TypeError::Mismatch {
                    expected: Type::func(vec![], Type::Unit),
                    got: arg_ty,
                    span: args[0].span,
                });
            }
            return Ok(Type::generic("ObjCBlock", vec![arg_ty]));
        }
        let cls = self.classes.get(&class).ok_or_else(|| TypeError::UndefinedClass {
            name: class.clone(),
            span,
        })?;
        if cls.extern_lib.is_some() {
            return Err(TypeError::Unsupported {
                what: format!(
                    "cannot construct opaque extern class {class:?} with `new` — \
                     values come from native extern fn return values"
                ),
                span,
            });
        }
        let class_params = cls.type_params.clone();
        // After the mangling pass, an overloaded `init` is renamed
        // to e.g. `init__i64`; New.init_method records which one
        // was picked. Look that up first so a re-typecheck on the
        // mangled program still resolves correctly.
        let init_lookup: Symbol = init_method.unwrap_or_else(|| "init".into());
        let init_raw = cls.methods.get(&init_lookup);
        // Generic instantiation: arity check on type args.
        if !class_params.is_empty() && type_args.len() != class_params.len() {
            return Err(TypeError::ArityMismatch {
                name: Symbol::intern(&format!("{class}::<type args>")),
                expected: class_params.len(),
                got: type_args.len(),
                span,
            });
        }
        // Non-generic class with explicit `<...>` args is an error.
        if class_params.is_empty() && !type_args.is_empty() {
            return Err(TypeError::ArityMismatch {
                name: Symbol::intern(&format!("{class}::<type args>")),
                expected: 0,
                got: type_args.len(),
                span,
            });
        }
        // `Set<T>` accepts the same primitives as Map's key plus
        // floats — the host runtime stores by bit pattern, so f32 /
        // f64 round-trip cleanly (NaN compares unequal to itself,
        // matching the IEEE rule but with the consequence that a
        // NaN inserted twice keeps two entries).
        if class.as_str() == "Map" && type_args.len() == 2 {
            let k = &type_args[0];
            if !super::super::sigs::is_valid_map_key_type(
                k,
                Some(&self.classes),
                Some(&self.enums),
            ) {
                let hint = match k {
                    Type::Object(c) => format!(
                        "map key type {k} — class {c:?} must declare \
                         `pub fn equals(other: {c:?}): bool` and \
                         `pub fn hashCode(): i64` (or carry `@derive(Eq, Hash)`)"
                    ),
                    _ => format!(
                        "map key type {k} (primitives, strings, or classes \
                         with `equals` + `hashCode` are supported)"
                    ),
                };
                return Err(TypeError::Unsupported { what: hint, span });
            }
        }
        if class.as_str() == "Set" && type_args.len() == 1 {
            let t = &type_args[0];
            if !super::super::sigs::is_valid_set_element_type(t, &self.classes, &self.enums) {
                let hint = match t {
                    Type::Object(c) => format!(
                        "set element type {t} — class {c:?} must declare \
                         `pub fn equals(other: {c:?}): bool` and \
                         `pub fn hashCode(): i64` (or carry `@derive(Eq, Hash)`)"
                    ),
                    _ => format!(
                        "set element type {t} (primitives, strings, or classes \
                         with `equals` + `hashCode` are supported)"
                    ),
                };
                return Err(TypeError::Unsupported { what: hint, span });
            }
        }
        // Rewrite the user-supplied type args so any reference to
        // an enclosing type parameter (a class param like `new
        // Map<string, T[]>()` inside `class Bag<T>`, OR a fn-level
        // param like `new Box<T>(v)` inside `fn make<T>(...)`)
        // lands as `TypeVar("T")` rather than the parser-default
        // `Object("T")`. Without this, the resulting Generic's
        // args wouldn't match a same-shape param/return type that
        // was already TypeVar-rewritten, and assignment / return
        // would fail with `expected Box<T>, got Box<T>` (the two
        // `T`s display the same but compare unequal).
        //
        // `current_type_params` holds class params + fn-own params
        // for the currently-checked fn body (set up by `check_fn`).
        let enclosing_params: Vec<Symbol> = self
            .current_type_params
            .borrow()
            .clone();
        let inst_args: Vec<Type> = type_args
            .iter()
            .map(|t| {
                if enclosing_params.is_empty() {
                    t.clone()
                } else {
                    rewrite_type_params(t, &enclosing_params)
                }
            })
            .collect();
        if let Some(init_overloads) = init_raw {
            // Substitute generic type-args once per init
            // overload, then resolve which init to call.
            let substituted: Vec<Signature> = init_overloads
                .iter()
                .map(|init| Signature {
                    params: init
                        .params
                        .iter()
                        .map(|t| subst_type(t, &class_params, &inst_args))
                        .collect(),
                    ret: subst_type(&init.ret, &class_params, &inst_args),
                    variadic: init.variadic,
                    type_params: Vec::new(),
                    decl_span: init.decl_span,
                    defaults: init.defaults.clone(),
                    is_pub: init.is_pub,
                    deprecated: init.deprecated.clone(), lib_names: init.lib_names.clone(),
                })
                .collect();
            let chosen = self.resolve_method_call(
                *class, "init".into(), &substituted, args, env, ret_ty, in_class, loop_depth, span, &[],
            )?;
            let cmod = cls.module.clone();
            self.require_visible(
                class.as_str(), &cmod, "init", "init", chosen.is_pub, span,
            )?;
        } else if !args.is_empty() {
            // C99 flexible array member: `@extern(C) struct`
            // ending in `T[]` accepts exactly one i64 arg
            // (the trailing element count) at construction.
            if cls.has_fam && args.len() == 1 {
                let t = self.check_expr(
                    &args[0], env, ret_ty, in_class, loop_depth,
                )?;
                if !matches!(
                    t,
                    Type::I8 | Type::I16 | Type::I32 | Type::I64
                    | Type::U8 | Type::U16 | Type::U32 | Type::U64
                ) {
                    return Err(TypeError::Mismatch {
                        expected: Type::I64,
                        got: t,
                        span: args[0].span,
                    });
                }
            } else {
                return Err(TypeError::ArityMismatch {
                    name: Symbol::intern(&format!("{class}::init")),
                    expected: 0,
                    got: args.len(),
                    span,
                });
            }
        }
        Ok(if class_params.is_empty() {
            Type::Object(class.clone())
        } else {
            Type::generic(class.clone(), inst_args)
        })
    }
}
