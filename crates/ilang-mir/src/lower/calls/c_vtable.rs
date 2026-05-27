//! `vtbl.Method(args)` where `vtbl: *T` and `T` is an @extern(C)
//! struct with a fn-typed `Method` field — COM vtable dispatch.

use ilang_ast::{Expr, ExprKind, Symbol};

use crate::inst::{FuncRef, Inst, ValueId};
use crate::types::MirTy;

use super::super::{BodyCx, LowerError};

impl<'a> BodyCx<'a> {
    /// COM vtable dispatch: `vtbl.Method(args)` where `vtbl` is
    /// `*T` and `T` is an @extern(C) struct with a fn-typed
    /// `Method` field. Returns `Ok(None)` when the receiver
    /// isn't a raw pointer to a CRepr/CPacked/CUnion struct, or
    /// when no fn-typed field by that name exists — the caller
    /// then falls through to the normal method-dispatch paths.
    ///
    /// The receiver's type must be discoverable *without* lowering
    /// (so the fall-through case doesn't double-emit instructions
    /// for `obj`). We peek through Var / cast / Paren forms; other
    /// shapes fall through unchanged.
    pub(super) fn try_lower_c_struct_vtable_call(
        &mut self,
        obj: &Expr,
        method: Symbol,
        args: &[Expr],
    ) -> Result<Option<(ValueId, MirTy)>, LowerError> {
        let inner_cid = self.peek_c_struct_ptr_class(obj)?;
        let Some(cid) = inner_cid else {
            return Ok(None);
        };
        let meta = self.class_meta.get(&cid).expect("class meta");
        let Some(&fid) = meta.field_ix.get(&method) else {
            return Err(LowerError::Other(format!(
                "no field `{method}` on c-struct class id #{}",
                cid.0
            )));
        };
        let fty = meta.field_ty.get(&fid).cloned().unwrap();
        let MirTy::Fn(ft) = fty else {
            return Err(LowerError::Other(format!(
                "field `{method}` on c-struct is not fn-typed (got {fty})"
            )));
        };
        let off = self.classes[cid.0 as usize]
            .c_field_offsets
            .get(fid.0 as usize)
            .copied()
            .ok_or_else(|| {
                LowerError::Other(format!("missing c_field_offset for `{method}`"))
            })?;
        // Type peek matched — lower the receiver now (this is the
        // first and only side-effecting lower_expr on `obj`).
        let (ov, _oty) = self.lower_expr(obj)?;
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
        let raw_fn_ty = MirTy::RawFn(ft.clone());
        let callee_v = self.fb.new_value(raw_fn_ty.clone());
        self.fb.push_inst(Inst::Cast {
            dst: callee_v,
            kind: crate::inst::CastKind::PtrIntCast,
            src: raw_u64,
        });
        let mut arg_vals = Vec::with_capacity(args.len());
        for (i, a) in args.iter().enumerate() {
            let (v, vty) = self.lower_expr(a)?;
            let coerced = match ft.params.get(i) {
                Some(t) if t != &vty => self.coerce(v, &vty, t, a.span)?,
                _ => v,
            };
            arg_vals.push(coerced);
        }
        let sig = crate::inst::FnSig {
            params: ft.params.clone(),
            ret: ft.ret.clone(),
            variadic: false,
        };
        let dst = if matches!(ft.ret, MirTy::Unit) {
            None
        } else {
            Some(self.fb.new_value(ft.ret.clone()))
        };
        self.fb.push_inst(Inst::CallRawIndirect {
            dst,
            callee: callee_v,
            sig,
            args: arg_vals.into_boxed_slice(),
        });
        Ok(Some((dst.unwrap_or_else(|| self.const_unit()), ft.ret.clone())))
    }

    /// Peek at an expression's static type to see whether it's
    /// `*T` where T is a CRepr/CPacked/CUnion struct. Returns
    /// `Some(class_id)` when it is. Used by the COM-vtable
    /// dispatch fast path to gate without committing side effects.
    fn peek_c_struct_ptr_class(
        &mut self,
        e: &Expr,
    ) -> Result<Option<crate::types::ClassId>, LowerError> {
        use crate::program::ClassRepr;
        let ty = match &e.kind {
            ExprKind::Var(name) => self.lookup_var(*name).map(|(_, t)| t),
            ExprKind::Cast { ty, .. } => Some(self.resolve_ty(ty)?),
            _ => None,
        };
        let Some(MirTy::RawPtr { inner, .. }) = ty else {
            return Ok(None);
        };
        let MirTy::Object(cid) = *inner else {
            return Ok(None);
        };
        let is_c_struct = matches!(
            self.classes[cid.0 as usize].repr,
            ClassRepr::CRepr | ClassRepr::CPacked | ClassRepr::CUnion
        );
        Ok(if is_c_struct { Some(cid) } else { None })
    }
}
