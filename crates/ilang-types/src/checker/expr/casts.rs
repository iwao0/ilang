//! `ExprKind::Cast` — value coercions, extracted from
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

impl TypeChecker {
    pub(super) fn check_cast(
        &self,
        inner: &Expr, ty: &Type,
        env: &Vars,
        ret_ty: Option<&Type>,
        in_class: Option<Symbol>,
        loop_depth: u32,
        span: Span,
    ) -> Result<Type, TypeError> {
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
        // `@com interface` is a raw COM-vtable handle at the ABI
        // level — an 8-byte pointer with no ARC header. Allow
        // bidirectional casts to/from `i64` and `*void` so callers
        // can lift the out-param value handed back by
        // `D3D12CreateDevice` (and friends) into the typed
        // interface, and lower it back when it has to flow through
        // a void-pointer FFI slot.
        let is_com_iface = |t: &Type| match t {
            Type::Object(name) => self
                .interfaces
                .get(name)
                .map(|sig| sig.is_com)
                .unwrap_or(false),
            _ => false,
        };
        let is_void_ptr = |t: &Type| matches!(
            t,
            Type::RawPtr { inner, .. } if matches!(**inner, Type::CVoid)
        );
        if (from == Type::I64 && is_com_iface(ty))
            || (is_com_iface(&from) && *ty == Type::I64)
            || (is_void_ptr(&from) && is_com_iface(ty))
            || (is_com_iface(&from) && is_void_ptr(ty))
        {
            return Ok(ty.clone());
        }
        // `@handle pub struct H {}` — same ABI shape as `@com
        // interface` (a bare pointer-sized opaque handle), so allow
        // the same set of escape-hatch casts. Lets the user lift an
        // i64-typed `wndProc` arg into a real `HWND` value, and
        // shuffle handles through `*void` FFI slots.
        let is_handle_obj = |t: &Type| match t {
            Type::Object(name) => self
                .classes
                .get(name)
                .map(|s| s.is_handle)
                .unwrap_or(false),
            _ => false,
        };
        if (from == Type::I64 && is_handle_obj(ty))
            || (is_handle_obj(&from) && *ty == Type::I64)
            || (is_void_ptr(&from) && is_handle_obj(ty))
            || (is_handle_obj(&from) && is_void_ptr(ty))
            || (is_handle_obj(&from) && is_handle_obj(ty))  // HMODULE→HINSTANCE etc.
        {
            return Ok(ty.clone());
        }
        // Raw C pointer ↔ i64 escape hatch — pointers are
        // bit-equivalent to a 64-bit address. Lets out-pointer
        // patterns work (read an opaque address from i64[],
        // hand it back to a `*Foo` parameter).
        let is_raw_ptr = |t: &Type| matches!(t, Type::RawPtr { .. });
        let is_ptr_int = |t: &Type| matches!(t, Type::I64 | Type::U64);
        if (is_raw_ptr(&from) && is_ptr_int(ty))
            || (is_ptr_int(&from) && is_raw_ptr(ty))
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
        // Raw pointer ↔ fn(...) — reinterprets a 64-bit address as
        // a C function pointer. Used to consume `GetProcAddress` /
        // `dlsym` / `GetProcAddress`-style results and call them
        // through their declared signature.
        //
        // The reverse direction (`fn(...)` → `*void`) lets a typed
        // fn handle be passed back to a C-style void-pointer slot
        // (e.g. an `LPVOID lpUserData` callback context).
        //
        // Both directions are inside @extern(C) only — raw fn ptrs
        // are not first-class outside FFI scope.  Calling a value
        // obtained this way goes through `Inst::CallRawIndirect`
        // (no closure env / no fn_ptr-from-offset-0 dereference);
        // see the MIR lowering for `Call(Cast(...), args)`.
        let is_fn = |t: &Type| matches!(t, Type::Fn(_));
        if *self.in_extern_c.borrow()
            && ((is_raw_ptr(&from) && is_fn(ty))
                || (is_fn(&from) && is_raw_ptr(ty)))
        {
            return Ok(ty.clone());
        }
        // Array → raw pointer — hands the array's data buffer
        // address to the C-ABI side. Element types must match
        // (or be `void` on the target). Used by the @objc
        // bridge to pass `simd.f32x2[]` as `const vector_float2 *`
        // and similar SIMD-array factories. Restricted to
        // inside an `@extern(C) {}` block so raw pointers stay
        // confined to FFI scope, just like the ptr↔ptr case.
        if let (Type::Array { elem: arr_elem, .. }, Type::RawPtr { inner: ptr_inner, .. }) =
            (&from, ty)
        {
            if *self.in_extern_c.borrow()
                && (arr_elem == ptr_inner || matches!(**ptr_inner, Type::CVoid))
            {
                return Ok(ty.clone());
            }
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
}

impl TypeChecker {
    pub(super) fn check_fn_expr(
        &self,
        params: &[Param], ret: &Option<Type>, body: &Block,
        env: &Vars,
        _ret_ty: Option<&Type>,
        in_class: Option<Symbol>,
        _loop_depth: u32,
        span: Span,
    ) -> Result<Type, TypeError> {
        for Param { ty, span: pspan, .. } in params {
            self.validate_type(ty, *pspan, &[])?;
        }
        if let Some(r) = ret {
            self.validate_type(r, span, &[])?;
        }
        // The enclosing fn's type params are in scope inside the
        // closure too. Rewrite `Object("T")` in param/ret
        // annotations to `TypeVar("T")` so they unify with the
        // outer fn's already-rewritten signature shapes (e.g.
        // a `__first_State<T>` payload binding flows into the
        // closure via a same-name capture).
        let outer_tps = self.current_type_params.borrow().clone();
        let rewrite = |t: &Type| -> Type {
            if outer_tps.is_empty() {
                t.clone()
            } else {
                crate::checker::sigs::rewrite_type_params(t, &outer_tps)
            }
        };
        // Closures capture outer locals by value. The body's
        // local env starts from the outer env so free vars
        // resolve, then params overlay.
        let mut inner: Vars = env.clone();
        for Param { name, ty, .. } in params {
            inner.insert(name.clone(), rewrite(ty));
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
        let expected = rewrite(&ret.clone().unwrap_or(Type::Unit));
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
            params.iter().map(|p| rewrite(&p.ty)).collect(),
            expected,
        ))
    }
}

impl TypeChecker {
    pub(super) fn check_array(
        &self,
        elements: &[Expr],
        env: &Vars,
        ret_ty: Option<&Type>,
        in_class: Option<Symbol>,
        loop_depth: u32,
        _span: Span,
    ) -> Result<Type, TypeError> {
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
}

impl TypeChecker {
    pub(super) fn check_map_lit(
        &self,
        entries: &[(Expr, Expr)],
        env: &Vars,
        ret_ty: Option<&Type>,
        in_class: Option<Symbol>,
        loop_depth: u32,
        _span: Span,
    ) -> Result<Type, TypeError> {
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
}
