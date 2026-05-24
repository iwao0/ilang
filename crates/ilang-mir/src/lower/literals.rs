//! Array / tuple / map literal + `obj.field` / `obj[idx]`
//! lowering on `BodyCx`.
//!
//! - `lower_array_literal` / `lower_array_literal_with_hint`:
//!   `[a, b, c]`. The hint variant lets a callsite (e.g.
//!   `let xs: u8[] = [1, 2, 3]`) widen / narrow each element to
//!   the declared type during lowering.
//! - `lower_tuple_literal`: `(a, b)`.
//! - `lower_map_literal`: `{ "k": v, ... }` — emits `__map_new`
//!   plus per-entry `__map_set` calls.
//! - `lower_index` / `lower_field`: read paths for indexed and
//!   field-access expressions. Bounds checks, weak-ptr peeks,
//!   property getters, and module-namespace lookups all route
//!   through here.

use ilang_ast::{Expr, ExprKind, Span, Symbol};

use crate::inst::{FuncRef, Inst, UnOp, ValueId};
use crate::types::{MirTy, SimdElem};

use super::utils::retain_if_heap;
use super::{BodyCx, LowerError};

impl<'a> BodyCx<'a> {
    pub(super) fn lower_map_literal(
        &mut self,
        entries: &[(Expr, Expr)],
    ) -> Result<(ValueId, MirTy), LowerError> {
        if entries.is_empty() {
            // Empty map literal isn't valid surface syntax (`{}`
            // parses as a block); emit a fallback Map<string, i64>
            // and let the binding annotation override.
            let key = MirTy::Str;
            let val = MirTy::I64;
            let ty = MirTy::Map {
                key: Box::new(key.clone()),
                val: Box::new(val.clone()),
            };
            let v = self.fb.new_value(ty.clone());
            self.fb.push_inst(Inst::NewMap {
                dst: v,
                key,
                val,
                entries: Box::new([]),
            });
            return Ok((v, ty));
        }
        let mut pairs = Vec::with_capacity(entries.len());
        let mut key_ty: Option<MirTy> = None;
        let mut val_ty: Option<MirTy> = None;
        for (k, v) in entries {
            let (kv, kty) = self.lower_expr(k)?;
            let (vv, vty) = self.lower_expr(v)?;
            let ek = key_ty.get_or_insert(kty.clone()).clone();
            let ev = val_ty.get_or_insert(vty.clone()).clone();
            let kv = if kty == ek {
                kv
            } else {
                self.coerce(kv, &kty, &ek, k.span)?
            };
            let vv = if vty == ev {
                vv
            } else {
                self.coerce(vv, &vty, &ev, v.span)?
            };
            pairs.push((kv, vv));
        }
        let key = key_ty.unwrap();
        let val = val_ty.unwrap();
        let ty = MirTy::Map {
            key: Box::new(key.clone()),
            val: Box::new(val.clone()),
        };
        let dst = self.fb.new_value(ty.clone());
        self.fb.push_inst(Inst::NewMap {
            dst,
            key,
            val,
            entries: pairs.into_boxed_slice(),
        });
        Ok((dst, ty))
    }

    pub(super) fn lower_array_literal_with_hint(
        &mut self,
        items: &[Expr],
        elem_hint: Option<MirTy>,
        len_hint: Option<usize>,
    ) -> Result<(ValueId, MirTy), LowerError> {
        if items.is_empty() {
            let elem = elem_hint.unwrap_or(MirTy::I64);
            let ty = MirTy::Array { elem: Box::new(elem.clone()), len: len_hint };
            let v = self.fb.new_value(ty.clone());
            self.fb.push_inst(Inst::NewArrayEmpty {
                dst: v,
                elem,
                fixed_len: len_hint,
            });
            return Ok((v, ty));
        }
        let mut elem_vals = Vec::with_capacity(items.len());
        let mut elem_ty: Option<MirTy> = elem_hint.clone();
        // SIMD elements are constructed from an inner array literal
        // (`[[0.0, 0.0], [0.5, 1.0], ...]` against `simd.f32x2[N]`).
        // `lower_expr` of the inner `[0.0, 0.0]` would build an
        // `f64[]` and there's no array → simd `coerce`; dispatch
        // directly to the SIMD-construction path before recursing.
        let simd_elem_hint = match &elem_hint {
            Some(MirTy::Simd { elem, lanes }) => Some((*elem, *lanes)),
            _ => None,
        };
        for it in items {
            let elem_is_fresh = self.is_fresh_object_expr(it);
            let (vv, vty) = if let (Some((selem, slanes)), ExprKind::Array(lane_items)) =
                (simd_elem_hint, &it.kind)
            {
                self.lower_simd_from_array_literal(lane_items, selem, slanes, it.span)?
            } else {
                self.lower_expr(it)?
            };
            let target = elem_ty.get_or_insert(vty.clone()).clone();
            let coerced = if target == vty {
                vv
            } else {
                self.coerce(vv, &vty, &target, it.span)?
            };
            // Mirror the no-hint path: aliased heap elements need
            // a +1 because host_release_array cascade-releases each
            // stored Object on drop.
            let is_heap = matches!(
                target,
                MirTy::Object(_)
                    | MirTy::Array { .. }
                    | MirTy::Tuple(_)
                    | MirTy::Map { .. }
                    | MirTy::Optional(_)
                    | MirTy::Fn(_)
                    | MirTy::Str
            );
            if is_heap && !elem_is_fresh {
                self.fb.push_inst(Inst::Retain { value: coerced });
            }
            elem_vals.push(coerced);
        }
        let elem = elem_ty.unwrap();
        let ty = MirTy::Array { elem: Box::new(elem.clone()), len: len_hint };
        let v = self.fb.new_value(ty.clone());
        self.fb.push_inst(Inst::NewArray {
            dst: v,
            elem,
            items: elem_vals.into_boxed_slice(),
        });
        Ok((v, ty))
    }

    /// Lower an inner `[a, b, ...]` against a SIMD type hint into a
    /// `NewSimd` value. Mirrors the let-stmt's `simd_ty` path so
    /// nested array literals (e.g. `simd.f32x2[N] = [[..], [..]]`)
    /// can construct each element directly without going through
    /// the general `[f64]` lowering + a non-existent array→simd
    /// coerce.
    pub(super) fn lower_simd_from_array_literal(
        &mut self,
        lane_items: &[Expr],
        elem: SimdElem,
        lanes: u32,
        span: Span,
    ) -> Result<(ValueId, MirTy), LowerError> {
        let _ = span;
        if lane_items.len() != lanes as usize {
            return Err(LowerError::Other(format!(
                "expected {} elements for simd.{}x{}, got {}",
                lanes,
                elem.name_prefix(),
                lanes,
                lane_items.len()
            )));
        }
        let lane_scalar = elem.as_scalar_mir();
        let mut lane_vals: Vec<ValueId> = Vec::with_capacity(lane_items.len());
        for it in lane_items.iter() {
            let (vv, vty) = self.lower_expr(it)?;
            let coerced = if vty == lane_scalar {
                vv
            } else {
                self.coerce(vv, &vty, &lane_scalar, it.span)?
            };
            lane_vals.push(coerced);
        }
        let simd_ty = MirTy::Simd { elem, lanes };
        let dst = self.fb.new_value(simd_ty.clone());
        self.fb.push_inst(Inst::NewSimd {
            dst,
            lanes: lane_vals.into_boxed_slice(),
        });
        Ok((dst, simd_ty))
    }

    pub(super) fn lower_array_literal(&mut self, items: &[Expr]) -> Result<(ValueId, MirTy), LowerError> {
        if items.is_empty() {
            // `[]` requires a type annotation; the let stmt's coerce
            // step would correct the element type. Fall back to i64
            // here; this is rare enough that letting it be obviously
            // wrong is fine for now (the binding's type annotation
            // path is the supported way).
            let ty = MirTy::Array { elem: Box::new(MirTy::I64), len: None };
            let v = self.fb.new_value(ty.clone());
            self.fb.push_inst(Inst::NewArrayEmpty {
                dst: v,
                elem: MirTy::I64,
                fixed_len: None,
            });
            return Ok((v, ty));
        }
        let mut elem_vals = Vec::with_capacity(items.len());
        let mut elem_ty: Option<MirTy> = None;
        for it in items {
            let elem_is_fresh = self.is_fresh_object_expr(it);
            let (vv, vty) = self.lower_expr(it)?;
            let ty = elem_ty.get_or_insert(vty.clone()).clone();
            let coerced = if ty == vty {
                vv
            } else {
                self.coerce(vv, &vty, &ty, it.span)?
            };
            // Array elements: each slot owns +1 because the array's
            // host_release_array cascade calls release_object on
            // every stored Object on drop. Fresh values already
            // come with +1 (transfer); aliased Vars don't, so we
            // bump rc here. Without this, `let xs = [a, a]` plus
            // the eventual array drop double-frees `a`.
            let is_heap = matches!(
                ty,
                MirTy::Object(_)
                    | MirTy::Array { .. }
                    | MirTy::Tuple(_)
                    | MirTy::Map { .. }
                    | MirTy::Optional(_)
                    | MirTy::Fn(_)
                    | MirTy::Str
            );
            if is_heap && !elem_is_fresh {
                self.fb.push_inst(Inst::Retain { value: coerced });
            }
            elem_vals.push(coerced);
        }
        let elem = elem_ty.unwrap();
        let ty = MirTy::Array { elem: Box::new(elem.clone()), len: None };
        let v = self.fb.new_value(ty.clone());
        self.fb.push_inst(Inst::NewArray {
            dst: v,
            elem,
            items: elem_vals.into_boxed_slice(),
        });
        Ok((v, ty))
    }

    pub(super) fn lower_tuple_literal(&mut self, items: &[Expr]) -> Result<(ValueId, MirTy), LowerError> {
        let mut vals = Vec::with_capacity(items.len());
        let mut tys = Vec::with_capacity(items.len());
        for it in items {
            let elem_is_fresh = self.is_fresh_object_expr(it);
            let (v, t) = self.lower_expr(it)?;
            // Tuple slots own their stored heap value's +1, mirroring
            // the array-literal element-retain rule. Without this,
            // `(read, bump)` over locals like `let read = fn(){...}`
            // would let the surrounding scope-exit release the
            // closure to rc=0 and free it while the tuple still
            // points there.
            let is_heap = matches!(
                t,
                MirTy::Object(_)
                    | MirTy::Array { .. }
                    | MirTy::Tuple(_)
                    | MirTy::Map { .. }
                    | MirTy::Optional(_)
                    | MirTy::Fn(_)
                    | MirTy::Str
            );
            if is_heap && !elem_is_fresh {
                self.fb.push_inst(Inst::Retain { value: v });
            }
            vals.push(v);
            tys.push(t);
        }
        let ty = MirTy::Tuple(tys.into_boxed_slice());
        let v = self.fb.new_value(ty.clone());
        self.fb.push_inst(Inst::NewTuple {
            dst: v,
            items: vals.into_boxed_slice(),
        });
        Ok((v, ty))
    }

    pub(super) fn lower_index(&mut self, obj: &Expr, index: &Expr) -> Result<(ValueId, MirTy), LowerError> {
        let obj_is_fresh = self.is_fresh_object_expr(obj);
        let (av, aty) = self.lower_expr(obj)?;
        match &aty {
            MirTy::Array { elem, .. } => {
                let elem_ty = (**elem).clone();
                let (iv, _) = self.lower_expr(index)?;
                let v = self.fb.new_value(elem_ty.clone());
                self.fb.push_inst(Inst::ArrayLoad { dst: v, arr: av, idx: iv });
                // Fresh-array index: retain the selected element so
                // the array's own Release (cascading deinit on every
                // stored Object) doesn't drop it. The unselected
                // elements get their deinits via the cascade.
                if obj_is_fresh && matches!(elem_ty, MirTy::Object(_)) {
                    self.fb.push_inst(Inst::Retain { value: v });
                    self.fb.push_inst(Inst::Release { value: av });
                }
                Ok((v, elem_ty))
            }
            MirTy::Map { val, .. } => {
                let val_ty = (**val).clone();
                let (kv, _) = self.lower_expr(index)?;
                let v = self.fb.new_value(val_ty.clone());
                self.fb.push_inst(Inst::MapGet { dst: v, map: av, key: kv });
                // `__map_get` (the runtime helper) already retains
                // heap values on read, so the caller always
                // receives a `+1` reference. For a fresh-receiver
                // index (`make_map()["k"]`) we just need to
                // release the soon-to-be-orphan map; no extra
                // retain on `v` (that would over-count and leak
                // the selected entry forever).
                if obj_is_fresh {
                    self.fb.push_inst(Inst::Release { value: av });
                }
                Ok((v, val_ty))
            }
            MirTy::Tuple(elems) => {
                let idx = match &index.kind {
                    ExprKind::Int(n) if *n >= 0 => *n as u32,
                    _ => {
                        return Err(LowerError::Other(
                            "tuple index must be a non-negative integer literal".into(),
                        ))
                    }
                };
                let elem_ty = elems
                    .get(idx as usize)
                    .cloned()
                    .ok_or_else(|| LowerError::Other(format!("tuple index {idx} out of range")))?;
                let v = self.fb.new_value(elem_ty.clone());
                self.fb.push_inst(Inst::TupleExtract { dst: v, tup: av, idx });
                // Fresh-tuple-on-index cleanup: extract may keep one
                // element alive (the selected one), but the others are
                // about to leak. Retain the selected Object so it
                // outlives the per-element release sweep, then release
                // every Object element of the fresh tuple.
                if obj_is_fresh {
                    if matches!(elem_ty, MirTy::Object(_)) {
                        self.fb.push_inst(Inst::Retain { value: v });
                    }
                    for (i, ety) in elems.iter().enumerate() {
                        if matches!(ety, MirTy::Object(_)) {
                            let ev = self.fb.new_value(ety.clone());
                            self.fb.push_inst(Inst::TupleExtract {
                                dst: ev,
                                tup: av,
                                idx: i as u32,
                            });
                            self.fb.push_inst(Inst::Release { value: ev });
                        }
                    }
                }
                Ok((v, elem_ty))
            }
            other => Err(LowerError::Other(format!("indexing non-indexable type {other}"))),
        }
    }

    pub(super) fn lower_field(
        &mut self,
        obj: &Expr,
        name: Symbol,
        _span: Span,
    ) -> Result<(ValueId, MirTy), LowerError> {
        // `typeof(x).name` — pseudo-property on the Type handle that
        // `typeof` returns. Lower obj (yields the dynamic class id),
        // then call `class_name` builtin to get the class name.
        if name.as_str() == "name" {
            if let ExprKind::Call { callee, args } = &obj.kind {
                if callee.as_str() == "typeof" && args.len() == 1 {
                    let (cid, _) = self.lower_expr(obj)?;
                    let v = self.fb.new_value(MirTy::Str);
                    self.fb.push_inst(Inst::Call {
                        dst: Some(v),
                        callee: FuncRef::Builtin(Symbol::intern("class_name")),
                        args: Box::new([cid]),
                    });
                    return Ok((v, MirTy::Str));
                }
            }
        }
        // `ClassName.field` — static access. The receiver is a bare
        // identifier that names a class, not an instance. Static
        // getter accessors take precedence over fields; the call
        // takes no arguments since there's no `this`.
        if let ExprKind::Var(maybe_class) = &obj.kind {
            if self.lookup_var(*maybe_class).is_none() {
                if let Some(cid) = super::class_id_by_name(self.classes, self.class_meta, *maybe_class) {
                    let meta = self.class_meta.get(&cid).unwrap();
                    if let Some((fid, prop_ty)) = meta.static_property_getter.get(&name).cloned() {
                        let v = self.fb.new_value(prop_ty.clone());
                        self.fb.push_inst(Inst::Call {
                            dst: Some(v),
                            callee: FuncRef::Local(fid),
                            args: Box::new([]),
                        });
                        return Ok((v, prop_ty));
                    }
                    if let Some(&slot) = meta.static_slots.get(&name) {
                        let slot_owner = &self.classes[cid.0 as usize];
                        let ty = self
                            .classes[cid.0 as usize]
                            .statics
                            .iter()
                            .find_map(|sid| {
                                let s = &self.statics_by_id(*sid);
                                if s.name == name {
                                    Some(s.ty.clone())
                                } else {
                                    None
                                }
                            })
                            .unwrap_or(MirTy::I64);
                        let _ = slot_owner;
                        let v = self.fb.new_value(ty.clone());
                        self.fb.push_inst(Inst::LoadStatic { dst: v, slot });
                        return Ok((v, ty));
                    }
                }
            }
        }
        let obj_is_fresh = self.is_fresh_object_expr(obj);
        let (ov, oty) = self.lower_expr(obj)?;
        // Property getter on an instance.
        if let MirTy::Object(cid) = &oty {
            let meta = self.class_meta.get(cid).expect("class meta");
            if let Some((mid, prop_ty)) = meta.property_getter.get(&name).cloned() {
                let v = self.fb.new_value(prop_ty.clone());
                self.fb.push_inst(Inst::Call {
                    dst: Some(v),
                    callee: FuncRef::Local(mid),
                    args: Box::new([ov]),
                });
                return Ok((v, prop_ty));
            }
        }
        // Built-in `.length` on arrays / strings.
        if name == "length" {
            match &oty {
                MirTy::Array { .. } => {
                    let v = self.fb.new_value(MirTy::I64);
                    self.fb.push_inst(Inst::ArrayLen { dst: v, arr: ov });
                    return Ok((v, MirTy::I64));
                }
                MirTy::Str => {
                    // String length is a runtime call (Unicode
                    // code-point count). Lower as a builtin.
                    let v = self.fb.new_value(MirTy::I64);
                    self.fb.push_inst(Inst::Call {
                        dst: Some(v),
                        callee: FuncRef::Builtin(Symbol::intern("str_length")),
                        args: Box::new([ov]),
                    });
                    return Ok((v, MirTy::I64));
                }
                _ => {}
            }
        }
        // Optional accessors (.isSome / .isNone).
        if let MirTy::Optional(_) = &oty {
            if name == "isSome" {
                let v = self.fb.new_value(MirTy::Bool);
                self.fb.push_inst(Inst::OptionalIsSome { dst: v, opt: ov });
                return Ok((v, MirTy::Bool));
            }
            if name == "isNone" {
                let s = self.fb.new_value(MirTy::Bool);
                self.fb.push_inst(Inst::OptionalIsSome { dst: s, opt: ov });
                let v = self.fb.new_value(MirTy::Bool);
                self.fb.push_inst(Inst::UnOp { dst: v, op: UnOp::BoolNot, src: s });
                return Ok((v, MirTy::Bool));
            }
        }
        // Class instance field.
        if let MirTy::Object(cid) = &oty {
            let meta = self.class_meta.get(cid).expect("class meta");
            if let Some(&fid) = meta.field_ix.get(&name) {
                let fty = meta.field_ty.get(&fid).cloned().unwrap();
                let v = self.fb.new_value(fty.clone());
                self.fb.push_inst(Inst::LoadField { dst: v, obj: ov, field: fid });
                // Release a fresh-receiver Object after extracting
                // a non-Object field — the receiver is otherwise
                // leaked. Heap-typed fields need a retain first
                // so the cascade triggered by `Release v` doesn't
                // tear the field down: the receiver owned a +1
                // on the field (the array / map / etc.), and once
                // the receiver's rc hits zero its
                // `__release_object_fields` cascade releases that
                // same +1. Without the retain, the caller gets
                // a dangling pointer.
                if obj_is_fresh && !matches!(fty, MirTy::Object(_)) {
                    retain_if_heap(&mut self.fb, v, &fty);
                    // @objc-class receivers are a special case:
                    // their `handle` field is the underlying
                    // refcounted ObjC pointer, and the wrapper's
                    // deinit calls `objc_release(handle)` to drop
                    // it. If the caller extracts `.handle` and
                    // hands it to something that uses it later
                    // in the same statement
                    // (e.g. `objcRetain(dev.newBuffer(…).handle)`),
                    // releasing the wrapper here means the ObjC
                    // object is freed before the consumer sees
                    // the pointer. Defer the release: register
                    // the receiver as an anonymous SSA binding in
                    // the current scope so the scope-exit pass
                    // picks it up, keeping the underlying ObjC
                    // object alive through the rest of the
                    // enclosing block.
                    //
                    // The @objc marker is the presence of a
                    // `handle: i64` field on the receiver class.
                    let is_objc_receiver = meta
                        .field_ix
                        .get(&Symbol::intern("handle"))
                        .and_then(|h_fid| meta.field_ty.get(h_fid))
                        .is_some_and(|t| matches!(t, MirTy::I64));
                    if is_objc_receiver {
                        let anon =
                            Symbol::intern("__field_receiver_temp");
                        self.env.bind(anon, ov, oty.clone());
                    } else {
                        self.fb.push_inst(Inst::Release { value: ov });
                    }
                }
                return Ok((v, fty));
            }
            return Err(LowerError::Other(format!(
                "no field `{name}` on class id #{}",
                cid.0
            )));
        }
        // `*T.field` on a raw pointer to an `@extern(C)` struct — COM
        // vtable dispatch. Read directly from `ptr + offset` using the
        // existing __read_u64 builtin; fn-typed fields surface as
        // `MirTy::RawFn(_)` so the call site picks up
        // `CallRawIndirect` for free.
        if let MirTy::RawPtr { inner, .. } = &oty {
            if let MirTy::Object(cid) = &**inner {
                let cls = &self.classes[cid.0 as usize];
                use crate::program::ClassRepr;
                let is_c_struct = matches!(cls.repr, ClassRepr::CRepr | ClassRepr::CPacked | ClassRepr::CUnion);
                if is_c_struct {
                    let meta = self.class_meta.get(cid).expect("class meta");
                    if let Some(&fid) = meta.field_ix.get(&name) {
                        let off = self.classes[cid.0 as usize]
                            .c_field_offsets
                            .get(fid.0 as usize)
                            .copied()
                            .ok_or_else(|| {
                                LowerError::Other(format!(
                                    "missing c_field_offset for `{name}`"
                                ))
                            })?;
                        let fty = meta.field_ty.get(&fid).cloned().unwrap();
                        // Coerce the raw ptr value to i64, then call
                        // __read_u64(addr, offset) to load the 8-byte
                        // slot. The reinterpret happens via the result
                        // value's type tag — no MIR cast needed for the
                        // bit pattern itself.
                        let addr = self.fb.new_value(MirTy::I64);
                        self.fb.push_inst(Inst::Cast {
                            dst: addr,
                            kind: crate::inst::CastKind::PtrIntCast,
                            src: ov,
                        });
                        let off_v = self.const_int(MirTy::I64, off);
                        let raw_u64 = self.fb.new_value(MirTy::U64);
                        self.fb.push_inst(Inst::Call {
                            dst: Some(raw_u64),
                            callee: FuncRef::Builtin(Symbol::intern("$ffi.readU64")),
                            args: Box::new([addr, off_v]),
                        });
                        // Re-tag the loaded u64 as the declared field
                        // type. For fn-typed fields we use RawFn so the
                        // call_fn lowering picks up CallRawIndirect; for
                        // any other supported field type we issue a
                        // PtrIntCast to widen/narrow into the target
                        // ABI shape (pointer or integer).
                        match fty {
                            MirTy::Fn(ft) => {
                                let out_ty = MirTy::RawFn(ft);
                                let out = self.fb.new_value(out_ty.clone());
                                self.fb.push_inst(Inst::Cast {
                                    dst: out,
                                    kind: crate::inst::CastKind::PtrIntCast,
                                    src: raw_u64,
                                });
                                return Ok((out, out_ty));
                            }
                            other => {
                                let out = self.fb.new_value(other.clone());
                                self.fb.push_inst(Inst::Cast {
                                    dst: out,
                                    kind: crate::inst::CastKind::PtrIntCast,
                                    src: raw_u64,
                                });
                                return Ok((out, other));
                            }
                        }
                    }
                    return Err(LowerError::Other(format!(
                        "no field `{name}` on c-struct class id #{}",
                        cid.0
                    )));
                }
            }
        }
        Err(LowerError::Other(format!(
            "field `{name}` on unsupported type {oty}"
        )))
    }
}
