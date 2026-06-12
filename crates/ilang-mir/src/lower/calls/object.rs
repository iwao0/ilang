//! Object instance method dispatch — interface (incl. @com),
//! fn-typed field calls, and class virtual / direct calls.
//! Also covers `Weak<T>.get()` since it shares the receiver
//! cascade-release logic with the object path.

use ilang_ast::{Expr, Symbol};

use crate::inst::{FuncRef, Inst, ValueId};
use crate::types::MirTy;

use super::super::{BodyCx, LowerError};

impl<'a> BodyCx<'a> {
    pub(super) fn try_lower_weak_method(
        &mut self,
        ov: ValueId,
        oty: &MirTy,
        method: Symbol,
        _args: &[Expr],
    ) -> Result<Option<(ValueId, MirTy)>, LowerError> {
        let MirTy::Weak(class_id) = oty else {
            return Ok(None);
        };
        if method.as_str() != "get" {
            return Ok(None);
        }
        let opt_ty = MirTy::Optional(Box::new(MirTy::Object(*class_id)));
        let dst = self.fb.new_value(opt_ty.clone());
        self.fb.push_inst(Inst::WeakUpgrade { dst, weak: ov });
        Ok(Some((dst, opt_ty)))
    }

    pub(super) fn try_lower_object_method(
        &mut self,
        ov: ValueId,
        oty: &MirTy,
        obj_is_fresh: bool,
        method: Symbol,
        args: &[Expr],
    ) -> Result<Option<(ValueId, MirTy)>, LowerError> {
        let MirTy::Object(class_id) = oty else {
            return Ok(None);
        };
        // Interface dispatch: when the static receiver type is an
        // interface, look the method's slot up in the global iface
        // table and emit a `VirtCall` against the receiver. The
        // runtime reads the receiver's actual class id from the
        // heap header and routes to the implementing class's fn
        // registered at this slot during class lowering.
        let iface_name = self
            .interface_ids
            .iter()
            .find_map(|(n, cid)| if cid == class_id { Some(*n) } else { None });
        if let Some(ifn) = iface_name {
            if self.com_interfaces.contains(&ifn) {
                return self
                    .lower_com_iface_dispatch(ov, ifn, method, args)
                    .map(Some);
            }
            return self
                .lower_iface_dispatch(ov, ifn, obj_is_fresh, method, args)
                .map(Some);
        }
        let meta = self.class_meta.get(class_id).expect("class meta");
        if !meta.method_ids.contains_key(&method) {
            // Fn-typed instance field — `obj.field(args)` becomes
            // LoadField + CallIndirect. Mirrors the type-checker
            // fallback in `crates/ilang-types/.../calls.rs`.
            if let Some(&fid) = meta.field_ix.get(&method) {
                let fty = meta.field_ty.get(&fid).cloned().unwrap();
                if let MirTy::Fn(ft) = fty.clone() {
                    return self
                        .lower_fn_field_call(ov, fid, &fty, &ft, obj_is_fresh, args)
                        .map(Some);
                }
            }
            return Err(LowerError::Other(
                format!("no method `{method}` on class"),
            ));
        }
        self.lower_class_method_call(ov, *class_id, obj_is_fresh, method, args)
            .map(Some)
    }

    fn lower_com_iface_dispatch(
        &mut self,
        ov: ValueId,
        ifn: Symbol,
        method: Symbol,
        args: &[Expr],
    ) -> Result<(ValueId, MirTy), LowerError> {
        let slot = self
            .com_iface_slots
            .get(&(ifn, method))
            .copied()
            .ok_or_else(|| {
                LowerError::Other(format!(
                    "@com interface `{ifn}` has no method `{method}`"
                ))
            })?;
        let sig = self
            .iface_method_sigs
            .get(&(ifn, method))
            .cloned()
            .ok_or_else(|| {
                LowerError::Other(format!(
                    "@com interface `{ifn}` method `{method}` has no recorded signature"
                ))
            })?;
        // Receiver becomes the first C ABI param (the COM `this`
        // pointer). Force the recv MIR type onto i64 so the
        // call_indirect sees a plain pointer.
        let recv_i64 = self.fb.new_value(MirTy::I64);
        self.fb.push_inst(Inst::Cast {
            dst: recv_i64,
            kind: crate::inst::CastKind::PtrIntCast,
            src: ov,
        });
        let mut com_sig_params: Vec<MirTy> = Vec::with_capacity(sig.params.len() + 1);
        com_sig_params.push(MirTy::I64);
        com_sig_params.extend(sig.params.iter().cloned());
        let mut user_args: Vec<ValueId> = Vec::with_capacity(args.len());
        for (i, a) in args.iter().enumerate() {
            let (coerced, _) = self.lower_arg_to(a, sig.params.get(i))?;
            user_args.push(coerced);
        }
        let dst = if matches!(sig.ret, MirTy::Unit) {
            None
        } else {
            Some(self.fb.new_value(sig.ret.clone()))
        };
        let com_sig = crate::inst::FnSig {
            params: com_sig_params.into_boxed_slice(),
            ret: sig.ret.clone(),
            variadic: false,
        };
        self.fb.push_inst(Inst::ComCall {
            dst,
            recv: recv_i64,
            slot,
            sig: com_sig,
            args: user_args.into_boxed_slice(),
        });
        Ok((dst.unwrap_or_else(|| self.const_unit()), sig.ret))
    }

    fn lower_iface_dispatch(
        &mut self,
        ov: ValueId,
        ifn: Symbol,
        obj_is_fresh: bool,
        method: Symbol,
        args: &[Expr],
    ) -> Result<(ValueId, MirTy), LowerError> {
        let slot = self
            .iface_method_slots
            .get(&(ifn, method))
            .copied()
            .ok_or_else(|| {
                LowerError::Other(format!(
                    "interface `{ifn}` has no method `{method}`"
                ))
            })?;
        let sig = self
            .iface_method_sigs
            .get(&(ifn, method))
            .cloned()
            .ok_or_else(|| {
                LowerError::Other(format!(
                    "interface `{ifn}` method `{method}` has no recorded signature"
                ))
            })?;
        let mut user_args: Vec<ValueId> = Vec::with_capacity(args.len());
        for (i, a) in args.iter().enumerate() {
            let (coerced, _) = self.lower_arg_to(a, sig.params.get(i))?;
            user_args.push(coerced);
        }
        let dst = if matches!(sig.ret, MirTy::Unit) {
            None
        } else {
            Some(self.fb.new_value(sig.ret.clone()))
        };
        self.fb.push_inst(Inst::VirtCall {
            dst,
            recv: ov,
            slot: crate::inst::VTableSlot(slot),
            args: user_args.into_boxed_slice(),
        });
        // Release a fresh intermediate receiver (`mkHolder().grab()` in
        // a chain) — including Object returns, which are owned (+1) just
        // like the direct-method path. No `@objc` exemption: an
        // ilang-interface VirtCall only reaches ilang classes (COM
        // interfaces take the separate `lower_com_iface_dispatch` path,
        // and @objc classes dispatch via objc_msgSend, not this vtable).
        if obj_is_fresh {
            self.fb.push_inst(Inst::Release { value: ov });
        }
        Ok((dst.unwrap_or_else(|| self.const_unit()), sig.ret))
    }

    fn lower_fn_field_call(
        &mut self,
        ov: ValueId,
        fid: crate::inst::FieldId,
        fty: &MirTy,
        ft: &crate::types::MirFnTy,
        obj_is_fresh: bool,
        args: &[Expr],
    ) -> Result<(ValueId, MirTy), LowerError> {
        let fn_val = self.fb.new_value(fty.clone());
        self.fb.push_inst(Inst::LoadField {
            dst: fn_val,
            obj: ov,
            field: fid,
        });
        let mut arg_vals = Vec::with_capacity(args.len());
        for (i, a) in args.iter().enumerate() {
            let (coerced, _) = self.lower_arg_to(a, ft.params.get(i))?;
            arg_vals.push(coerced);
        }
        let dst = if matches!(ft.ret, MirTy::Unit) {
            None
        } else {
            Some(self.fb.new_value(ft.ret.clone()))
        };
        let call_sig = crate::inst::FnSig {
            params: ft.params.clone(),
            ret: ft.ret.clone(),
            variadic: false,
        };
        self.fb.push_inst(Inst::CallIndirect {
            dst,
            callee: fn_val,
            sig: call_sig,
            args: arg_vals.into_boxed_slice(),
        });
        // Release a fresh receiver whose closure-field we just called
        // (`makeObj().fnField()`). The result is the CLOSURE's return —
        // an owned (+1) value, never an alias of the receiver — so this
        // is safe even for Object returns (the `!Object` guard here
        // leaked the fresh receiver the same way the direct-method path
        // did).
        if obj_is_fresh {
            self.fb.push_inst(Inst::Release { value: ov });
        }
        Ok((dst.unwrap_or_else(|| self.const_unit()), ft.ret.clone()))
    }

    fn lower_class_method_call(
        &mut self,
        ov: ValueId,
        class_id: crate::types::ClassId,
        obj_is_fresh: bool,
        method: Symbol,
        args: &[Expr],
    ) -> Result<(ValueId, MirTy), LowerError> {
        let meta = self.class_meta.get(&class_id).expect("class meta");
        let mid = *meta.method_ids.get(&method).unwrap();
        let sig = meta.method_sigs.get(&method).cloned().unwrap();
        let slot = self.classes[class_id.0 as usize]
            .methods
            .iter()
            .find(|m| m.name == method)
            .and_then(|m| m.slot);

        let mut arg_vals_all = Vec::with_capacity(args.len() + 1);
        arg_vals_all.push(ov);
        let mut fresh_obj_args: Vec<ValueId> = Vec::new();
        for (i, a) in args.iter().enumerate() {
            let arg_is_fresh = self.is_fresh_object_expr(a);
            let (coerced, vty) = self.lower_arg_to(a, sig.params.get(i + 1))?;
            // Same fresh-transfer rule as `lower_new`: every heap arg
            // whose field-assign path retains (Object / Fn / Array /
            // Tuple / Map / Optional / Str) needs a post-call release
            // when the caller passed a fresh transient. Without it,
            // `obj.method(fresh_heap)` leaks one cell per call
            // whenever the method stashes the arg into a field (or
            // any callee path that retains).
            let needs_post_release = Self::fresh_arg_needs_post_release(&vty);
            if (arg_is_fresh || self.last_arg_wrapped) && needs_post_release {
                fresh_obj_args.push(coerced);
            }
            arg_vals_all.push(coerced);
        }
        let dst = if matches!(sig.ret, MirTy::Unit) {
            None
        } else {
            Some(self.fb.new_value(sig.ret.clone()))
        };
        if let Some(slot) = slot {
            let user_args: Box<[ValueId]> = arg_vals_all[1..].to_vec().into_boxed_slice();
            self.fb.push_inst(Inst::VirtCall {
                dst,
                recv: ov,
                slot,
                args: user_args,
            });
        } else {
            self.fb.push_inst(Inst::Call {
                dst,
                callee: FuncRef::Local(mid),
                args: arg_vals_all.into_boxed_slice(),
            });
        }
        for fv in fresh_obj_args {
            self.fb.push_inst(Inst::Release { value: fv });
        }
        // Release a fresh receiver that nothing else owns (e.g. the
        // intermediate `a.m1()` in a `a.m1().m2()` chain). For an
        // Object-returning method this also fires when the receiver is
        // a regular ilang class: a `get(): T { this.f }` return is
        // OWNED (+1 — the tail-borrow / bare-`this` retain), so the
        // result keeps its own share and survives the receiver's drop
        // cascade. Without this, every chained method call whose
        // intermediate returns an Object leaked that intermediate (and
        // everything it transitively owned). An `@objc` receiver is
        // exempt: its Object return is an autoreleased BORROW, not an
        // owned +1, so releasing the receiver would free the chain's
        // value out from under it (`NSURLComponents.alloc().init()`).
        // The @objc marker is a `handle: i64` field on the receiver
        // class — same probe `lower_field` uses for its receiver temp.
        let recv_is_objc = self
            .class_meta
            .get(&class_id)
            .and_then(|m| {
                m.field_ix
                    .get(&Symbol::intern("handle"))
                    .and_then(|h| m.field_ty.get(h))
            })
            .is_some_and(|t| matches!(t, MirTy::I64));
        let skip_release =
            matches!(sig.ret, MirTy::Object(_)) && recv_is_objc;
        if obj_is_fresh && !skip_release {
            self.fb.push_inst(Inst::Release { value: ov });
        }
        Ok((dst.unwrap_or_else(|| self.const_unit()), sig.ret))
    }
}
