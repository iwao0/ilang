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
            // The parser produces `StructLit`, but normalize desugars
            // it into `{ let __sl = new Foo(); __sl.f = v; ...; __sl }`
            // before type checking runs — reaching here means a
            // pipeline shortcut bypassed normalize.
            ExprKind::StructLit { .. } => Err(TypeError::Unsupported {
                what: "internal: struct literal reached type checker (normalize was skipped)".into(),
                span,
            }),
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
                let sigs = self.fns.get(fn_name).cloned().ok_or_else(|| {
                    TypeError::UndefinedVariable {
                        name: fn_name.clone(),
                        span,
                    }
                })?;
                let sig = sigs.into_iter().next().expect("registered fn has sig");
                let user_params: Vec<Type> = sig.params.iter().skip(1).cloned().collect();
                Ok(Type::func(user_params, sig.ret))
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
                let parent_sig = self.classes.get(&parent_name).cloned().expect("parent registered");
                let lookup: Symbol = method.unwrap_or_else(|| "init".into());
                let sigs = parent_sig.methods.get(&lookup).cloned().ok_or_else(|| {
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
                let sig = sigs.into_iter().next().unwrap();
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
                }
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
                if callee == "deinit" {
                    return Err(TypeError::CannotCallDeinit { span });
                }
                // Built-in `typeof(x): Type` — RTTI introspection.
                // Accepts any single value; the JIT / interpreter
                // synthesise the right Type metadata at runtime.
                if callee == "typeof" {
                    if args.len() != 1 {
                        return Err(TypeError::ArityMismatch {
                            name: callee.clone(),
                            expected: 1,
                            got: args.len(),
                            span,
                        });
                    }
                    self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
                    return Ok(Type::Object("Type".into()));
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
                        defaults: Vec::new(), is_pub: true };
                    self.check_args(*callee, &sig, args, env, ret_ty, in_class, loop_depth, span)?;
                    return Ok(sig.ret);
                }
                if let Some(class_name) = in_class {
                    if let Some(cls) = self.classes.get(&class_name) {
                        if let Some(sigs) = cls.methods.get(callee).cloned() {
                            // Implicit-this method call. Resolve overload
                            // exactly like a top-level fn call.
                            let chosen = self.resolve_method_call(
                                class_name, *callee, &sigs, args, env, ret_ty, in_class, loop_depth, span,
                            )?;
                            return Ok(chosen.ret);
                        }
                    }
                }
                let sigs = self.fns.get(callee).cloned().ok_or_else(|| {
                    TypeError::UndefinedFunction {
                        name: callee.clone(),
                        span,
                    }
                })?;
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
                        let sig = sigs.into_iter().next().unwrap();
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
                        &sigs,
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
                let sig = sigs.into_iter().next().unwrap();
                // Generic fn — see below; we also stash the inferred
                // type-args vector keyed by call span so the JIT's
                // monomorphization pass can find it later.
                // Generic fn: infer type-arg bindings from the (parametric
                // param type, arg type) pairs, then validate arg-by-arg
                // against the substituted param types and return the
                // substituted return type. Mirrors enum-ctor inference.
                if sig.params.len() != args.len() {
                    return Err(TypeError::ArityMismatch {
                        name: callee.clone(),
                        expected: sig.params.len(),
                        got: args.len(),
                        span,
                    });
                }
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
            ExprKind::Field { obj, name } => {
                // Static field read: `ClassName.field` when there's
                // no shadowing local and the class declares a
                // static field by that name.
                if let ExprKind::Var(rname) = &obj.kind {
                    let is_local_shadow = env.contains_key(rname) || self.vars.contains_key(rname);
                    if !is_local_shadow {
                        if let Some(cls) = self.classes.get(&rname) {
                            if let Some(t) = cls.static_fields.get(name) {
                                let is_pub = cls.static_field_pub.get(name).copied().unwrap_or(false);
                                let cmod = cls.module.clone();
                                let cn = rname.as_str().to_string();
                                self.require_visible(
                                    &cn, &cmod, "static field", name.as_str(), is_pub, span,
                                )?;
                                return Ok(t.clone());
                            }
                        }
                    }
                }
                let ot = self.check_expr(obj, env, ret_ty, in_class, loop_depth)?;
                // Built-in property: every array exposes `length: i64`.
                if matches!(ot, Type::Array { .. }) && name == "length" {
                    return Ok(Type::I64);
                }
                // Built-in property: strings expose `length: i64` (Unicode
                // code-point count, JS-style).
                if matches!(ot, Type::Str) && name == "length" {
                    return Ok(Type::I64);
                }
                // Built-in Optional properties: `isSome` / `isNone`.
                if matches!(ot, Type::Optional(_))
                    && (name == "isSome" || name == "isNone")
                {
                    return Ok(Type::Bool);
                }
                // Built-in Result properties: `isOk` / `isErr`.
                if (name == "isOk" || name == "isErr") && is_result_type(&ot) {
                    return Ok(Type::Bool);
                }
                // Built-in RTTI: `Type.name` / `Type.kind` / `Type.parent`.
                if matches!(&ot, Type::Object(n) if n.as_str() == "Type") {
                    if name == "name" {
                        return Ok(Type::Str);
                    }
                    if name == "kind" {
                        return Ok(Type::Object("TypeKind".into()));
                    }
                    if name == "parent" {
                        return Ok(Type::Optional(Box::new(Type::Object("Type".into()))));
                    }
                    if name == "fields" || name == "methods" {
                        return Ok(Type::Array { elem: Box::new(Type::Str), fixed: None });
                    }
                    if name == "typeArgs" {
                        return Ok(Type::Array {
                            elem: Box::new(Type::Object("Type".into())),
                            fixed: None,
                        });
                    }
                }
                let class_name = expect_object(&ot, span)?;
                let cls = self.classes.get(&class_name).ok_or_else(|| {
                    TypeError::UndefinedClass {
                        name: class_name.into(),
                        span,
                    }
                })?;
                // Property `get` takes precedence over field lookup —
                // the parser disallows declaring a property and a
                // same-named field on one class, but checking properties
                // first keeps the resolution explicit.
                if let Some(p) = cls.properties.get(name) {
                    if !p.has_get {
                        return Err(TypeError::Unsupported {
                            what: format!(
                                "property {:?}.{} has no getter (write-only)",
                                class_name, name
                            ),
                            span,
                        });
                    }
                    let cmod = cls.module.clone();
                    self.require_visible(
                        class_name.as_str(), &cmod, "property", name.as_str(), p.is_pub, span,
                    )?;
                    return Ok(subst_type(
                        &p.ty,
                        &cls.type_params,
                        type_args_of(&ot),
                    ));
                }
                let raw = cls.fields.get(name).cloned().ok_or_else(|| {
                    TypeError::UnknownField {
                        class: class_name.into(),
                        field: name.clone(),
                        span,
                    }
                })?;
                // `@extern(C) struct` fields are transparent C ABI
                // bridges — there's no private state to protect, so
                // skip the per-field visibility check on them.
                if !cls.is_repr_c {
                    let is_pub = cls.field_pub.get(name).copied().unwrap_or(false);
                    let cmod = cls.module.clone();
                    self.require_visible(
                        class_name.as_str(), &cmod, "field", name.as_str(), is_pub, span,
                    )?;
                }
                Ok(subst_type(&raw, &cls.type_params, type_args_of(&ot)))
            }
            ExprKind::MethodCall { obj, method, args } => {
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
                        if let Some(cls) = self.classes.get(&name) {
                            if let Some(sigs) = cls.static_methods.get(method).cloned() {
                                let cmod = cls.module.clone();
                                let cn = name.as_str().to_string();
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
                                    loop_depth, span,
                                )?;
                                return Ok(chosen.ret);
                            }
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
                        if args.len() != arity {
                            return Err(TypeError::ArityMismatch {
                                name: method.clone(),
                                expected: arity,
                                got: args.len(),
                                span,
                            });
                        }
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
                        if !args.is_empty() {
                            return Err(TypeError::ArityMismatch {
                                name: "get".into(),
                                expected: 0,
                                got: args.len(),
                                span,
                            });
                        }
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
                            if !args.is_empty() {
                                return Err(TypeError::ArityMismatch {
                                    name: method.clone(),
                                    expected: 0,
                                    got: args.len(),
                                    span,
                                });
                            }
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
                // Built-in `.toString()` for numeric primitives and
                // `bool`. Decimal for ints, JS-style for floats
                // (matching `console.log`'s formatting), `"true"` /
                // `"false"` for bool.
                if (ot.is_numeric() || ot == Type::Bool) && method.as_str() == "toString" {
                    if !args.is_empty() {
                        return Err(TypeError::ArityMismatch {
                            name: method.clone(),
                            expected: 0,
                            got: args.len(),
                            span,
                        });
                    }
                    return Ok(Type::Str);
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
                        if args.len() != 1 {
                            return Err(TypeError::ArityMismatch {
                                name: "push".into(),
                                expected: 1,
                                got: args.len(),
                                span,
                            });
                        }
                        let at = self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
                        if !self.value_assignable(&args[0], &at, elem) {
                            return Err(TypeError::Mismatch {
                                expected: (**elem).clone(),
                                got: at,
                                span: args[0].span,
                            });
                        }
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
                        if !args.is_empty() {
                            return Err(TypeError::ArityMismatch {
                                name: "pop".into(),
                                expected: 0,
                                got: args.len(),
                                span,
                            });
                        }
                        return Ok(Type::Optional(elem.clone()));
                    }
                    if method == "indexOf" || method == "includes" {
                        if args.len() != 1 {
                            return Err(TypeError::ArityMismatch {
                                name: method.clone(),
                                expected: 1,
                                got: args.len(),
                                span,
                            });
                        }
                        let at = self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
                        if !self.value_assignable(&args[0], &at, elem) {
                            return Err(TypeError::Mismatch {
                                expected: (**elem).clone(),
                                got: at,
                                span: args[0].span,
                            });
                        }
                        return Ok(if method == "indexOf" {
                            Type::I64
                        } else {
                            Type::Bool
                        });
                    }
                    if method == "slice" {
                        // slice(start: i64, end: i64): T[]
                        if args.len() != 2 {
                            return Err(TypeError::ArityMismatch {
                                name: "slice".into(),
                                expected: 2,
                                got: args.len(),
                                span,
                            });
                        }
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
                    if method == "map" || method == "filter" || method == "forEach" {
                        if args.len() != 1 {
                            return Err(TypeError::ArityMismatch {
                                name: method.clone(),
                                expected: 1,
                                got: args.len(),
                                span,
                            });
                        }
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
                        return Ok(match method.as_str() {
                            "map" => Type::Array { elem: Box::new(ret), fixed: None },
                            "filter" => {
                                if !matches!(ret, Type::Bool) {
                                    return Err(TypeError::Mismatch {
                                        expected: Type::Bool,
                                        got: ret,
                                        span: args[0].span,
                                    });
                                }
                                Type::Array { elem: elem.clone(), fixed: None }
                            }
                            "forEach" => Type::Unit,
                            _ => unreachable!(),
                        });
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
                            if args.len() != 1 {
                                return Err(TypeError::ArityMismatch {
                                    name: "has".into(),
                                    expected: 1,
                                    got: args.len(),
                                    span,
                                });
                            }
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
                let class_name = expect_object(&ot, span)?;
                // Receiver typed as an interface: look the method up
                // on the interface itself; runtime resolves the
                // implementing fn from the receiver's actual class.
                if let Some(isig) = self.interfaces.get(&class_name).cloned() {
                    if let Some(im) = isig.methods.iter().find(|m| m.name == *method) {
                        let sig = Signature {
                            params: im.params.clone(),
                            ret: im.ret.clone(),
                            variadic: false,
                            type_params: Vec::new(),
                            decl_span: span,
                            defaults: vec![None; im.params.len()],
                            is_pub: true,
                        };
                        let chosen = self.resolve_method_call(
                            class_name.into(), *method, &[sig], args, env, ret_ty, in_class,
                            loop_depth, span,
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
                let raw_sigs = cls.methods.get(method).cloned().ok_or_else(|| {
                    TypeError::UnknownMethod {
                        class: class_name.into(),
                        method: method.clone(),
                        span,
                    }
                })?;
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
                        type_params: Vec::new(),
                        decl_span: raw.decl_span,
                        defaults: raw.defaults.clone(),
                        is_pub: raw.is_pub,
                    })
                    .collect();
                // At least one overload must be reachable from the
                // current module for the call to be legal. We check
                // pub on the chosen overload after resolution to
                // surface a precise error.
                let chosen = self.resolve_method_call(
                    class_name_owned, *method, &substituted, args, env, ret_ty, in_class, loop_depth, span,
                )?;
                let cmod = cls.module.clone();
                self.require_visible(
                    class_name.as_str(), &cmod, "method", method.as_str(), chosen.is_pub, span,
                )?;
                Ok(chosen.ret)
            }
            ExprKind::New { class, type_args, args, init_method } => {
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
                let init_raw = cls.methods.get(&init_lookup).cloned();
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
                let inst_args: Vec<Type> = type_args.to_vec();
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
                        })
                        .collect();
                    let chosen = self.resolve_method_call(
                        *class, "init".into(), &substituted, args, env, ret_ty, in_class, loop_depth, span,
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
            ExprKind::Block(b) => self.check_block(b, env, ret_ty, in_class, loop_depth),
            ExprKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
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
                let body_ty = body_res?;
                if body_ty != Type::Unit {
                    return Err(TypeError::Mismatch {
                        expected: Type::Unit,
                        got: body_ty,
                        span,
                    });
                }
                Ok(Type::Unit)
            }
            ExprKind::Loop { body } => {
                self.loop_stack.borrow_mut().push(LoopFrame::Loop(None));
                let body_res = self.check_block(body, env, ret_ty, in_class, loop_depth + 1);
                let frame = self.loop_stack.borrow_mut().pop();
                let body_ty = body_res?;
                if body_ty != Type::Unit {
                    return Err(TypeError::Mismatch {
                        expected: Type::Unit,
                        got: body_ty,
                        span,
                    });
                }
                // The loop's own type is the unified break-value type, or
                // Unit if no `break v` was seen.
                let break_ty = match frame {
                    Some(LoopFrame::Loop(Some(t))) => t,
                    _ => Type::Unit,
                };
                self.loop_break_type
                    .borrow_mut()
                    .insert(span, break_ty.clone());
                Ok(break_ty)
            }
            ExprKind::ForIn { var, iter, body } => {
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
                let body_ty = body_res?;
                if body_ty != Type::Unit {
                    return Err(TypeError::Mismatch {
                        expected: Type::Unit,
                        got: body_ty,
                        span,
                    });
                }
                Ok(Type::Unit)
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
                if elements.is_empty() {
                    // Element type is unknown; surface a marker
                    // (`Any`-element array) and let `literal_assignable`
                    // accept it when the let / param annotation pins the
                    // type. Bare `let a = []` falls through to the
                    // EmptyArrayNeedsAnnotation error in `check_stmt`.
                    return Ok(Type::Array {
                        elem: Box::new(Type::Any),
                        fixed: Some(0),
                    });
                }
                let mut elem_ty =
                    self.check_expr(&elements[0], env, ret_ty, in_class, loop_depth)?;
                for e in &elements[1..] {
                    let et = self.check_expr(e, env, ret_ty, in_class, loop_depth)?;
                    if self.value_assignable(e, &et, &elem_ty) {
                        continue;
                    }
                    // Heterogeneous classes: lift `elem_ty` to the
                    // common ancestor so `[new Circle(...), new Square(...)]`
                    // unifies to `Shape[]` (matches the if/else arm
                    // unification path). The elements are then re-
                    // checked against the lifted type so any further
                    // siblings still join cleanly.
                    if let (Type::Object(a), Type::Object(b)) = (&elem_ty, &et) {
                        if let Some(anc) = self.common_ancestor(*a, *b) {
                            elem_ty = Type::Object(anc);
                            continue;
                        }
                    }
                    // Mixed-length nested arrays: `[[a, b], [c]]`
                    // gives elements of type `T[2]` and `T[1]`. The
                    // outer array doesn't care about the inner
                    // length — drop the `fixed` marker and recurse
                    // through the element type's common-ancestor
                    // lift.
                    if let (
                        Type::Array { elem: ea, .. },
                        Type::Array { elem: eb, .. },
                    ) = (&elem_ty, &et) {
                        let inner = if ea == eb {
                            Some((**ea).clone())
                        } else if let (
                            Type::Object(ca),
                            Type::Object(cb),
                        ) = (ea.as_ref(), eb.as_ref())
                        {
                            self.common_ancestor(*ca, *cb).map(Type::Object)
                        } else {
                            None
                        };
                        if let Some(inner) = inner {
                            elem_ty = Type::Array {
                                elem: Box::new(inner),
                                fixed: None,
                            };
                            continue;
                        }
                    }
                    return Err(TypeError::Mismatch {
                        expected: elem_ty.clone(),
                        got: et,
                        span: e.span,
                    });
                }
                Ok(Type::Array {
                    elem: Box::new(elem_ty),
                    fixed: Some(elements.len()),
                })
            }
            ExprKind::Tuple(elements) => {
                let mut tys = Vec::with_capacity(elements.len());
                for e in elements {
                    tys.push(self.check_expr(e, env, ret_ty, in_class, loop_depth)?);
                }
                Ok(Type::Tuple(tys.into()))
            }
            ExprKind::MapLit(entries) => {
                // The parser only ever emits MapLit when there's at least
                // one `key: value` entry; `{}` parses as an empty block.
                let (k0, v0) = &entries[0];
                let k_ty = self.check_expr(k0, env, ret_ty, in_class, loop_depth)?;
                if !is_valid_map_key_type(&k_ty) {
                    return Err(TypeError::Unsupported {
                        what: format!(
                            "map key type {k_ty} (only string / int / bool keys are supported)"
                        ),
                        span: k0.span,
                    });
                }
                let mut v_ty = self.check_expr(v0, env, ret_ty, in_class, loop_depth)?;
                for (k, v) in &entries[1..] {
                    let kt = self.check_expr(k, env, ret_ty, in_class, loop_depth)?;
                    if !self.value_assignable(k, &kt, &k_ty) {
                        return Err(TypeError::Mismatch {
                            expected: k_ty.clone(),
                            got: kt,
                            span: k.span,
                        });
                    }
                    let vt = self.check_expr(v, env, ret_ty, in_class, loop_depth)?;
                    if self.value_assignable(v, &vt, &v_ty) {
                        continue;
                    }
                    // Heterogeneous class values: lift `v_ty` to the
                    // common ancestor so `{"a": new Circle(), "b": new
                    // Square()}` infers `Map<string, Shape>` —
                    // matches the array-literal / branch unification
                    // behaviour.
                    if let (Type::Object(a), Type::Object(b)) = (&v_ty, &vt) {
                        if let Some(anc) = self.common_ancestor(*a, *b) {
                            v_ty = Type::Object(anc);
                            continue;
                        }
                    }
                    return Err(TypeError::Mismatch {
                        expected: v_ty.clone(),
                        got: vt,
                        span: v.span,
                    });
                }
                Ok(Type::generic("Map", vec![k_ty, v_ty]))
            }
            ExprKind::Index { obj, index } => {
                let ot = self.check_expr(obj, env, ret_ty, in_class, loop_depth)?;
                let it = self.check_expr(index, env, ret_ty, in_class, loop_depth)?;
                // Map<K, V> indexing: `m[k]` returns V (panics at runtime
                // if missing — use `.get(k)` for `V?`).
                if let Type::Generic(g) = &ot {
                    if g.base == "Map" && g.args.len() == 2 {
                        if !self.value_assignable(index, &it, &g.args[0]) {
                            return Err(TypeError::Mismatch {
                                expected: g.args[0].clone(),
                                got: it,
                                span: index.span,
                            });
                        }
                        return Ok(g.args[1].clone());
                    }
                }
                // Tuple indexing: index must be a non-negative integer
                // literal so the element type is statically known.
                if let Type::Tuple(elems) = &ot {
                    let n = match &index.kind {
                        ExprKind::Int(n) if *n >= 0 => *n as usize,
                        _ => {
                            return Err(TypeError::Unsupported {
                                what: "tuple index must be a non-negative integer literal".into(),
                                span: index.span,
                            });
                        }
                    };
                    if n >= elems.len() {
                        return Err(TypeError::Unsupported {
                            what: format!(
                                "tuple index {n} out of bounds for {ot}"
                            ),
                            span: index.span,
                        });
                    }
                    return Ok(elems[n].clone());
                }
                if !it.is_int() {
                    return Err(TypeError::Mismatch {
                        expected: Type::I64,
                        got: it,
                        span: index.span,
                    });
                }
                match ot {
                    Type::Array { elem, .. } => Ok((*elem).clone()),
                    other => Err(TypeError::Mismatch {
                        expected: Type::Array {
                            elem: Box::new(Type::Any),
                            fixed: None,
                        },
                        got: other,
                        span: obj.span,
                    }),
                }
            }
            ExprKind::AssignIndex { obj, index, value } => {
                let ot = self.check_expr(obj, env, ret_ty, in_class, loop_depth)?;
                let it = self.check_expr(index, env, ret_ty, in_class, loop_depth)?;
                // Map<K, V>: `m[k] = v` desugars to `set(k, v)`.
                if let Type::Generic(g) = &ot {
                    if g.base == "Map" && g.args.len() == 2 {
                        if !self.value_assignable(index, &it, &g.args[0]) {
                            return Err(TypeError::Mismatch {
                                expected: g.args[0].clone(),
                                got: it,
                                span: index.span,
                            });
                        }
                        let vt = self.check_expr(value, env, ret_ty, in_class, loop_depth)?;
                        if !self.value_assignable(value, &vt, &g.args[1]) {
                            return Err(TypeError::Mismatch {
                                expected: g.args[1].clone(),
                                got: vt,
                                span: value.span,
                            });
                        }
                        return Ok(Type::Unit);
                    }
                }
                if !it.is_int() {
                    return Err(TypeError::Mismatch {
                        expected: Type::I64,
                        got: it,
                        span: index.span,
                    });
                }
                let elem_ty = match &ot {
                    Type::Array { elem, .. } => (**elem).clone(),
                    other => {
                        return Err(TypeError::Mismatch {
                            expected: Type::Array {
                                elem: Box::new(Type::Any),
                                fixed: None,
                            },
                            got: other.clone(),
                            span: obj.span,
                        });
                    }
                };
                let vt = self.check_expr(value, env, ret_ty, in_class, loop_depth)?;
                if !self.value_assignable(value, &vt, &elem_ty) {
                    return Err(TypeError::Mismatch {
                        expected: elem_ty,
                        got: vt,
                        span: value.span,
                    });
                }
                Ok(Type::Unit)
            }
            ExprKind::FnExpr { params, ret, body } => {
                for Param { ty, span: pspan, .. } in params {
                    self.validate_type(ty, *pspan, &[])?;
                }
                if let Some(r) = ret {
                    self.validate_type(r, span, &[])?;
                }
                // Closures capture outer locals by value. The body's
                // local env starts from the outer env so free vars
                // resolve, then params overlay.
                let mut inner: Vars = env.clone();
                for Param { name, ty, .. } in params {
                    inner.insert(name.clone(), ty.clone());
                }
                // Compute captures: free vars in the body that come
                // from the OUTER `env` (not the closure's own params,
                // not top-level fns/classes/enums). Order is
                // first-encountered for stable JIT layout.
                let mut bound: std::collections::HashSet<Symbol> =
                    params.iter().map(|p| p.name.clone()).collect();
                let mut frees: Vec<Symbol> = Vec::new();
                let mut seen: std::collections::HashSet<Symbol> = Default::default();
                collect_fn_expr_free_vars(body, &mut bound, &mut frees, &mut seen);
                let captures: Vec<(Symbol, Type)> = frees
                    .into_iter()
                    // Built-in singletons (`console`) live in the
                    // top-level env but the JIT has no class layout
                    // for `Console` — they're not user-capturable.
                    // Recognising them here keeps `console.log(...)`
                    // inside a closure body off the capture list, so
                    // both the interp ("globals") and JIT ("intercept
                    // at method-call site") paths see the same
                    // free-variable set.
                    .filter(|n| n.as_str() != "console")
                    .filter_map(|n| env.get(&n).map(|t| (n, t.clone())))
                    .collect();
                self.fn_expr_captures
                    .borrow_mut()
                    .insert(span, captures);
                // If we're inside a method body and the closure body
                // directly mentions `this`, record the lexical class
                // so the JIT hoist pass can promote `this` to a
                // synthetic capture. (Captures via inner closures
                // are handled transitively — each FnExpr records its
                // own direct `this` use only.)
                if let Some(class_name) = in_class {
                    if block_uses_this_directly(body) {
                        self.fn_expr_this_class
                            .borrow_mut()
                            .insert(span, class_name);
                    }
                }
                let expected = ret.clone().unwrap_or(Type::Unit);
                let body_ty =
                    self.check_block(body, &inner, Some(&expected), in_class, 0)?;
                let tail_check = body
                    .tail
                    .as_deref()
                    .map(|t| self.value_assignable(t, &body_ty, &expected));
                let ok = match tail_check {
                    Some(true) => true,
                    Some(false) => false,
                    None => {
                        assignable(&body_ty, &expected)
                            || self.assignable_obj(&body_ty, &expected)
                    }
                };
                if !ok {
                    return Err(TypeError::BadReturn {
                        name: "<closure>".into(),
                        expected,
                        got: body_ty,
                        span,
                    });
                }
                Ok(Type::func(
                    params.iter().map(|p| p.ty.clone()).collect(),
                    ret.clone().unwrap_or(Type::Unit),
                ))
            }
            ExprKind::Cast { expr: inner, ty } => {
                let from = self.check_expr(inner, env, ret_ty, in_class, loop_depth)?;
                self.validate_type(ty, span, &[])?;
                // Numeric → numeric (any width) and `bool → int`
                // (0/1 conversion) are the regular path.
                let from_ok = from.is_numeric() || from == Type::Bool;
                let to_ok = ty.is_numeric();
                if from_ok && to_ok {
                    return Ok(ty.clone());
                }
                // Enum → numeric: hand back the variant's
                // discriminant value as a primitive integer.
                // Mainly useful for fieldless enums with an
                // explicit `: u32` repr (bitflag-style usage —
                // `Flag.audio as u32 | Flag.video as u32`).
                if matches!(from, Type::Object(ref n) if self.enums.contains_key(n)) && ty.is_numeric() {
                    return Ok(ty.clone());
                }
                // Enum → string: only valid for `: string`-repr
                // enums (e.g. SDL hint name groups). Returns the
                // declared discriminant string. Cast emits a
                // runtime lookup against the per-variant table.
                if let Type::Object(ref n) = from {
                    if matches!(ty, Type::Str) {
                        if let Some(sig) = self.enums.get(n) {
                            if matches!(sig.repr, Some(Type::Str)) {
                                return Ok(Type::Str);
                            }
                        }
                    }
                }
                // Numeric → enum: reinterpret an integer as one of
                // the enum's discriminants. Only allowed for
                // fieldless (unit-variant-only) enums; payloaded
                // enums have no integer representation. Lets C-side
                // out values (`SDL_GetKeyFromScancode(...) as
                // Keycode`) round-trip into the typed enum.
                if from.is_numeric() {
                    if let Type::Object(n) = ty {
                        if let Some(sig) = self.enums.get(n) {
                            let fieldless = sig.variants.iter().all(|v| {
                                matches!(v.payload, VariantPayloadSig::Unit)
                            });
                            if fieldless {
                                return Ok(ty.clone());
                            }
                        }
                    }
                }
                // FFI escape hatch — `i64 ↔ opaque-extern class
                // (without deinit)`. Lets out-pointer slots from C
                // be reinterpreted as an opaque handle and vice
                // versa. Restricted to the deinit-less form so the
                // user never accidentally constructs a phantom ARC
                // box. Cast direction must come from the user (no
                // implicit conversion elsewhere) so it stays
                // explicit at every call site.
                let opaque_no_deinit = |t: &Type| match t {
                    Type::Object(name) => self
                        .classes
                        .get(name)
                        .map(|cs| {
                            cs.extern_lib.is_some() && !cs.methods.contains_key(&"deinit".into())
                        })
                        .unwrap_or(false),
                    _ => false,
                };
                if (from == Type::I64 && opaque_no_deinit(ty))
                    || (opaque_no_deinit(&from) && *ty == Type::I64)
                {
                    return Ok(ty.clone());
                }
                // Raw C pointer ↔ i64 escape hatch — pointers are
                // bit-equivalent to a 64-bit address. Lets out-pointer
                // patterns work (read an opaque address from i64[],
                // hand it back to a `*Foo` parameter).
                let is_raw_ptr = |t: &Type| matches!(t, Type::RawPtr { .. });
                if (is_raw_ptr(&from) && *ty == Type::I64)
                    || (from == Type::I64 && is_raw_ptr(ty))
                {
                    return Ok(ty.clone());
                }
                // Raw pointer ↔ raw pointer — type-punning at the
                // C boundary (`*const u8` → `*const void`,
                // `*const char` → `*u8`, etc.). All raw pointers are
                // i64-sized at the ABI; this just reinterprets the
                // pointee type. Restricted to inside `@extern(C) {}`
                // since raw pointer values aren't supposed to surface
                // outside the block in the first place.
                if is_raw_ptr(&from) && is_raw_ptr(ty) && *self.in_extern_c.borrow() {
                    return Ok(ty.clone());
                }
                // Class subtype upcast: `b as A` where `B extends A`
                // — always safe and explicit, so accept. The
                // narrowing direction (parent → child) is reserved
                // for `as?`, which returns `T?` to capture the
                // possible failure.
                if let (Type::Object(c), Type::Object(p)) = (&from, ty) {
                    if self.is_subclass(*c, *p) {
                        return Ok(ty.clone());
                    }
                }
                Err(TypeError::Mismatch {
                    expected: ty.clone(),
                    got: from,
                    span,
                })
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
                // Static field write: `ClassName.field = v`.
                if let ExprKind::Var(rname) = &obj.kind {
                    let is_local_shadow = env.contains_key(rname) || self.vars.contains_key(rname);
                    if !is_local_shadow {
                        if let Some(cls) = self.classes.get(&rname) {
                            if let Some(ft) = cls.static_fields.get(field).cloned() {
                                if cls.static_const_fields.contains(field) && !*is_init {
                                    return Err(TypeError::Unsupported {
                                        what: format!(
                                            "cannot assign to const static field {:?}.{:?}",
                                            rname, field
                                        ),
                                        span,
                                    });
                                }
                                let is_pub = cls.static_field_pub.get(field).copied().unwrap_or(false);
                                let cmod = cls.module.clone();
                                let cn = rname.as_str().to_string();
                                self.require_visible(
                                    &cn, &cmod, "static field", field.as_str(), is_pub, span,
                                )?;
                                let vt =
                                    self.check_expr(value, env, ret_ty, in_class, loop_depth)?;
                                if !self.value_assignable(value, &vt, &ft) {
                                    return Err(TypeError::Mismatch {
                                        expected: ft,
                                        got: vt,
                                        span: value.span,
                                    });
                                }
                                return Ok(Type::Unit);
                            }
                        }
                    }
                }
                let ot = self.check_expr(obj, env, ret_ty, in_class, loop_depth)?;
                let class_name = expect_object(&ot, obj.span)?;
                let cls = self.classes.get(&class_name).ok_or_else(|| {
                    TypeError::UndefinedClass {
                        name: class_name.into(),
                        span: obj.span,
                    }
                })?;
                // Property `set` precedes field lookup. Read-only
                // properties (no setter) reject the assignment.
                if let Some(p) = cls.properties.get(field) {
                    if !p.has_set {
                        return Err(TypeError::Unsupported {
                            what: format!(
                                "property {:?}.{} has no setter (read-only)",
                                class_name, field
                            ),
                            span,
                        });
                    }
                    let cmod = cls.module.clone();
                    self.require_visible(
                        class_name.as_str(), &cmod, "property", field.as_str(), p.is_pub, span,
                    )?;
                    let prop_ty =
                        subst_type(&p.ty, &cls.type_params, type_args_of(&ot));
                    let v_ty =
                        self.check_expr(value, env, ret_ty, in_class, loop_depth)?;
                    if !self.value_assignable(value, &v_ty, &prop_ty) {
                        return Err(TypeError::Mismatch {
                            expected: prop_ty,
                            got: v_ty,
                            span: value.span,
                        });
                    }
                    return Ok(Type::Unit);
                }
                let raw_field_ty = cls.fields.get(field).cloned().ok_or_else(|| {
                    TypeError::UnknownField {
                        class: class_name.into(),
                        field: field.clone(),
                        span,
                    }
                })?;
                if !cls.is_repr_c {
                    let is_pub = cls.field_pub.get(field).copied().unwrap_or(false);
                    let cmod = cls.module.clone();
                    self.require_visible(
                        class_name.as_str(), &cmod, "field", field.as_str(), is_pub, span,
                    )?;
                }
                // Substitute the receiver's generic type args so a
                // `Box<i64>.x = 100` check sees `i64` for `x: T`.
                // Mirrors the substitution done by the Field read path.
                let field_ty = subst_type(&raw_field_ty, &cls.type_params, type_args_of(&ot));
                let v_ty = self.check_expr(value, env, ret_ty, in_class, loop_depth)?;
                if !self.value_assignable(value, &v_ty, &field_ty) {
                    return Err(TypeError::Mismatch {
                        expected: field_ty,
                        got: v_ty,
                        span: value.span,
                    });
                }
                Ok(Type::Unit)
            }
            ExprKind::None => Ok(Type::Optional(Box::new(Type::Any))),
            ExprKind::Some(inner) => {
                let it = self.check_expr(inner, env, ret_ty, in_class, loop_depth)?;
                Ok(Type::Optional(Box::new(it)))
            }
            ExprKind::IfLet {
                name,
                expr,
                then_branch,
                else_branch,
            } => {
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
            ExprKind::EnumCtor {
                enum_name,
                variant,
                args,
            } => {
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
            ExprKind::Match { scrutinee, arms } => {
                let st = self.check_expr(scrutinee, env, ret_ty, in_class, loop_depth)?;
                // Match on a primitive (integer / bool / string)
                // is allowed, with `IntLit` / `BoolLit` / `StrLit`
                // patterns. Bool literals appear as
                // `Variant{name: "true"|"false"}` from the parser,
                // which we treat as `BoolLit` here.
                if st.is_numeric() || st == Type::Bool || st == Type::Str {
                    return self.check_match_primitive(&st, arms, span, env, ret_ty, in_class, loop_depth);
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
                    result_ty = Some(match result_ty {
                        None => bt,
                        Some(prev) => self.unify_branch_obj(prev, bt, arm.body.span)?,
                    });
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
    }

}
