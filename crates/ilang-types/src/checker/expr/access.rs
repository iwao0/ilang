//! Field / AssignField / Index / AssignIndex — read / write
//! access on object, array, and map values. Extracted from
//! `expr/mod.rs`.

#![allow(unused_imports)]

use std::collections::{HashMap, HashSet};

use ilang_ast::{
    Block, ClassDecl, CtorArgs, EnumDecl, Expr, ExprKind, FieldDecl, FnDecl, Item, Param,
    PatternBindings, PatternKind, Program, Span, Stmt, StmtKind, Symbol, Type, UnOp,
    VariantPayload,
};

use crate::error::TypeError;
use crate::ops::{assignable, bin_result, int_literal_fits};

use super::super::*;

/// Returns `Some(Type::F32)` / `Some(Type::F64)` when `receiver.name`
/// names one of the built-in float associated constants
/// (`f32.NaN`, `f64.MinPositive`, ...). The set mirrors Rust's
/// `f32::NAN` / `f32::INFINITY` / `f32::NEG_INFINITY` / `f32::MIN`
/// / `f32::MAX` / `f32::MIN_POSITIVE` / `f32::EPSILON` (and the f64
/// twins). Names are CamelCase to match ilang's identifier style.
pub(crate) fn float_prim_const_type(receiver: &str, name: &str) -> Option<Type> {
    let is_const = matches!(
        name,
        "NaN" | "Infinity" | "NegInfinity"
            | "Min" | "Max" | "MinPositive" | "Epsilon"
    );
    if !is_const {
        return None;
    }
    match receiver {
        "f32" => Some(Type::F32),
        "f64" => Some(Type::F64),
        _ => None,
    }
}

/// Returns `Some(Type::I8)` / etc. when `receiver.name` names a
/// signed / unsigned integer's `Min` / `Max` associated constant
/// (`i32.Min`, `u8.Max`, ...). Values come from Rust's `i*::MIN`
/// / `i*::MAX`; signedness picks the natural bounds.
pub(crate) fn int_prim_const_type(receiver: &str, name: &str) -> Option<Type> {
    if !matches!(name, "Min" | "Max") {
        return None;
    }
    match receiver {
        "i8" => Some(Type::I8),
        "i16" => Some(Type::I16),
        "i32" => Some(Type::I32),
        "i64" => Some(Type::I64),
        "u8" => Some(Type::U8),
        "u16" => Some(Type::U16),
        "u32" => Some(Type::U32),
        "u64" => Some(Type::U64),
        _ => None,
    }
}

impl TypeChecker {
    pub(super) fn check_field(
        &self,
        obj: &Expr, name: &Symbol,
        env: &Vars,
        ret_ty: Option<&Type>,
        in_class: Option<Symbol>,
        loop_depth: u32,
        span: Span,
    ) -> Result<Type, TypeError> {
        // Static field / static getter read: `ClassName.name` when
        // there's no shadowing local and the class declares either a
        // static field or a `pub static get name(): T` accessor.
        // Static getters take precedence over fields (mirrors the
        // instance-side property-vs-field precedence below).
        if let ExprKind::Var(rname) = &obj.kind {
            let is_local_shadow = env.contains_key(rname) || self.vars.contains_key(rname);
            if !is_local_shadow {
                // Float primitives expose a small set of associated
                // constants (`f32.NaN`, `f32.Infinity`, ...). Names
                // mirror Rust's `f32::*` constants in CamelCase.
                if let Some(prim_ty) = float_prim_const_type(rname.as_str(), name.as_str()) {
                    return Ok(prim_ty);
                }
                if let Some(prim_ty) = int_prim_const_type(rname.as_str(), name.as_str()) {
                    return Ok(prim_ty);
                }
                if let Some(cls) = self.classes.get(&rname) {
                    if let Some(p) = cls.properties.get(name) {
                        if p.is_static {
                            if !p.has_get {
                                return Err(TypeError::Unsupported {
                                    what: format!(
                                        "static property {}.{} has no getter (write-only)",
                                        rname, name
                                    ),
                                    span,
                                });
                            }
                            let cmod = cls.module.clone();
                            self.require_visible(
                                rname.as_str(), &cmod, "static property",
                                name.as_str(), p.is_pub, span,
                            )?;
                            return Ok(p.ty.clone());
                        }
                    }
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
        // `*T` field read on a CRepr struct pointer: surface the
        // field's declared type directly. fn-typed fields then
        // dispatch via `CallRawIndirect` at the call site. No ARC
        // bookkeeping because the receiver is a raw C pointer (no
        // header, no retained reference). `@com interface` is the
        // preferred surface for COM vtables; this raw-pointer path
        // remains for hand-rolled vtable layouts that don't fit
        // the interface model.
        if let Type::RawPtr { inner, .. } = &ot {
            if let Type::Object(struct_name) = &**inner {
                if let Some(cls) = self.classes.get(struct_name) {
                    if cls.is_repr_c {
                        if let Some(raw) = cls.fields.get(name).cloned() {
                            return Ok(raw);
                        }
                        return Err(TypeError::UnknownField {
                            class: (*struct_name).into(),
                            field: name.clone(),
                            span,
                        });
                    }
                }
            }
        }
        let class_name = expect_object(&ot, span)?;
        // @objc-interface-typed receivers don't carry their own
        // field list (the interface is just a method contract),
        // but every Cocoa-protocol value is backed by an NSObject
        // and exposes the standard `handle: i64`. Recognise that
        // one field so the @objc dispatch wrappers can extract
        // `arg.handle` from an interface-typed param without
        // hitting an "undefined class" error.
        if self.interfaces.contains_key(&class_name) {
            if name.as_str() == "handle" {
                return Ok(Type::I64);
            }
            return Err(TypeError::UnknownField {
                class: class_name.into(),
                field: name.clone(),
                span,
            });
        }
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
}

impl TypeChecker {
    pub(super) fn check_assign_field(
        &self,
        obj: &Expr, field: &Symbol, value: &Expr, is_init: &bool,
        env: &Vars,
        ret_ty: Option<&Type>,
        in_class: Option<Symbol>,
        loop_depth: u32,
        span: Span,
    ) -> Result<Type, TypeError> {
        // Static getter / static field write: `ClassName.name = v`.
        // A `pub static set name(v: T)` takes precedence; a read-only
        // static property rejects the assignment up front.
        if let ExprKind::Var(rname) = &obj.kind {
            let is_local_shadow = env.contains_key(rname) || self.vars.contains_key(rname);
            if !is_local_shadow {
                if let Some(cls) = self.classes.get(&rname) {
                    if let Some(p) = cls.properties.get(field).cloned() {
                        if p.is_static {
                            if !p.has_set {
                                return Err(TypeError::Unsupported {
                                    what: format!(
                                        "static property {}.{} has no setter (read-only)",
                                        rname, field
                                    ),
                                    span,
                                });
                            }
                            let cmod = cls.module.clone();
                            self.require_visible(
                                rname.as_str(), &cmod, "static property",
                                field.as_str(), p.is_pub, span,
                            )?;
                            let vt = self.check_expr(value, env, ret_ty, in_class, loop_depth)?;
                            if !self.value_assignable(value, &vt, &p.ty) {
                                return Err(TypeError::Mismatch {
                                    expected: p.ty.clone(),
                                    got: vt,
                                    span: value.span,
                                });
                            }
                            return Ok(Type::Unit);
                        }
                    }
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
}

impl TypeChecker {
    pub(super) fn check_index(
        &self,
        obj: &Expr, index: &Expr,
        env: &Vars,
        ret_ty: Option<&Type>,
        in_class: Option<Symbol>,
        loop_depth: u32,
        _span: Span,
    ) -> Result<Type, TypeError> {
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
}

impl TypeChecker {
    pub(super) fn check_assign_index(
        &self,
        obj: &Expr, index: &Expr, value: &Expr,
        env: &Vars,
        ret_ty: Option<&Type>,
        in_class: Option<Symbol>,
        loop_depth: u32,
        _span: Span,
    ) -> Result<Type, TypeError> {
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
}
