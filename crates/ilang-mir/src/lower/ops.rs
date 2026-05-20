//! Unary / binary / short-circuit logical operator lowering on
//! `BodyCx`.
//!
//! `lower_unary` covers `-x` / `+x` / `!x` / `~x`. `lower_binary`
//! handles arithmetic, bit-wise, shift, comparison, range and
//! `string + string` ops, calling `unify_numeric` to align operand
//! widths first. `lower_logical` builds the short-circuit `||` /
//! `&&` control flow with a fresh join block.

use ilang_ast::{BinOp as AstBinOp, Expr, ExprKind as AstExprKind, LogicalOp, Span, Symbol, UnOp as AstUnOp};

use crate::inst::{BinOp, FuncRef, Inst, MirConst, Terminator, UnOp, ValueId};
use crate::types::MirTy;

use super::env::Binding;
use super::utils::{cmp_op, Cmp};
use super::{BodyCx, LowerError};

impl<'a> BodyCx<'a> {
    pub(super) fn lower_unary(&mut self, op: AstUnOp, e: &Expr, span: Span) -> Result<(ValueId, MirTy), LowerError> {
        // `&path` is special: the operand is never evaluated as a
        // value — it's an lvalue chain (Var optionally followed by
        // field accesses). The leaf becomes the address-of target.
        if matches!(op, AstUnOp::AddrOf) {
            return self.lower_addr_of_path(e, span);
        }
        let (v, ty) = self.lower_expr(e)?;
        match op {
            AstUnOp::Pos => Ok((v, ty)),
            AstUnOp::Neg => {
                let dst = self.fb.new_value(ty.clone());
                let mop = if ty.is_int() { UnOp::INeg } else { UnOp::FNeg };
                self.fb.push_inst(Inst::UnOp { dst, op: mop, src: v });
                Ok((dst, ty))
            }
            AstUnOp::Not => {
                let dst = self.fb.new_value(MirTy::Bool);
                self.fb.push_inst(Inst::UnOp { dst, op: UnOp::BoolNot, src: v });
                Ok((dst, MirTy::Bool))
            }
            AstUnOp::BitNot => {
                let dst = self.fb.new_value(ty.clone());
                self.fb.push_inst(Inst::UnOp { dst, op: UnOp::Not, src: v });
                Ok((dst, ty))
            }
            AstUnOp::AddrOf => unreachable!("handled above"),
        }
    }

    /// Lower `&path` where `path` is a `Var` optionally followed by
    /// a field chain. For the chain case, intermediate field hops
    /// emit `LoadField` (which loads the class pointer for ARC
    /// fields or returns the inline base for CRepr struct fields);
    /// the final hop emits `AddrOfField` to compute the storage
    /// address. The bare `&local` case emits `AddrOfLocal`, which
    /// pins the local into a Cranelift stack slot.
    fn lower_addr_of_path(
        &mut self,
        e: &Expr,
        _span: Span,
    ) -> Result<(ValueId, MirTy), LowerError> {
        let mut fields_rev: Vec<Symbol> = Vec::new();
        let mut cur: &Expr = e;
        loop {
            match &cur.kind {
                AstExprKind::Var(n) => {
                    let root_name = *n;
                    fields_rev.reverse();
                    return self.lower_addr_of_decomposed(root_name, &fields_rev);
                }
                AstExprKind::This => {
                    fields_rev.reverse();
                    // `this` is registered under the canonical symbol
                    // "this" inside method bodies — lower like a
                    // regular Var, letting the existing param /
                    // capture / local lookup handle the rest.
                    let this_sym = Symbol::intern("this");
                    return self.lower_addr_of_decomposed(this_sym, &fields_rev);
                }
                AstExprKind::Field { obj, name } => {
                    fields_rev.push(*name);
                    cur = obj;
                }
                _ => {
                    return Err(LowerError::Other(
                        "`&` target must be a local variable, `this`, or a field chain"
                            .to_string(),
                    ));
                }
            }
        }
    }

    fn lower_addr_of_decomposed(
        &mut self,
        root_name: Symbol,
        fields: &[Symbol],
    ) -> Result<(ValueId, MirTy), LowerError> {
        // Bare `&name` requires a mutable local — we need a stable
        // stack slot. Field chains (`&name.f...`) just need the
        // binding's *current value* (the class pointer / inline
        // buffer pointer), so plain SSA values (e.g., function
        // parameters) work too.
        if fields.is_empty() {
            // Short-circuit for CRepr `Object` bindings — the value
            // is already a pointer to the C struct's storage, so
            // `&s` is a bitcast from `Object(N)` to `*N` (not a
            // stack-slot pin). Works for both `let`-bound locals
            // and SSA bindings (fn params, returned values).
            let crepr_short = match self.env.lookup_binding(root_name) {
                Some(Binding::Local(lid, t)) => {
                    if let MirTy::Object(cid) = &t {
                        if matches!(
                            self.classes[cid.0 as usize].repr,
                            crate::program::ClassRepr::CRepr
                                | crate::program::ClassRepr::CPacked
                                | crate::program::ClassRepr::CUnion
                        ) {
                            let v = self.fb.new_value(t.clone());
                            self.fb.push_inst(Inst::UseLocal { dst: v, local: lid });
                            Some((v, t))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                Some(Binding::Ssa(v, t)) => {
                    if let MirTy::Object(cid) = &t {
                        if matches!(
                            self.classes[cid.0 as usize].repr,
                            crate::program::ClassRepr::CRepr
                                | crate::program::ClassRepr::CPacked
                                | crate::program::ClassRepr::CUnion
                        ) {
                            Some((v, t))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if let Some((src_v, src_ty)) = crepr_short {
                let ptr_ty = MirTy::RawPtr {
                    is_const: false,
                    inner: Box::new(src_ty),
                };
                let dst = self.fb.new_value(ptr_ty.clone());
                self.fb.push_inst(Inst::Cast {
                    dst,
                    kind: crate::inst::CastKind::PtrCast,
                    src: src_v,
                });
                return Ok((dst, ptr_ty));
            }
            // Top-level `pub const` whose RHS couldn't compile-time
            // fold (e.g. an `@extern(C)` struct literal like
            // `pub const IID_X: GUID = GUID { ... }`) gets demoted by
            // the loader into a runtime module-level `let`, which
            // shows up here as a `repl_slot` reference rather than a
            // function-scope binding. Load the slot once into a fresh
            // SSA value and reuse the CRepr fast path so `&CONST`
            // works at FFI call sites without a manual
            // `let tmp = CONST` dance. Currently only CRepr Objects
            // are handled — other slot shapes would need a real
            // stack-slot copy, see HANDOFF note.
            if let Some((idx, slot_ty)) = self.repl_slots.get(&root_name).cloned() {
                if let MirTy::Object(cid) = &slot_ty {
                    if matches!(
                        self.classes[cid.0 as usize].repr,
                        crate::program::ClassRepr::CRepr
                            | crate::program::ClassRepr::CPacked
                            | crate::program::ClassRepr::CUnion
                    ) {
                        let idx_v = self.const_int(MirTy::I64, idx as i64);
                        let raw = self.fb.new_value(MirTy::I64);
                        self.fb.push_inst(Inst::Call {
                            dst: Some(raw),
                            callee: FuncRef::Builtin(Symbol::intern("__repl_load_slot")),
                            args: Box::new([idx_v]),
                        });
                        let src_v = self.i64_to_slot_value(raw, &slot_ty)?;
                        let ptr_ty = MirTy::RawPtr {
                            is_const: false,
                            inner: Box::new(slot_ty),
                        };
                        let dst = self.fb.new_value(ptr_ty.clone());
                        self.fb.push_inst(Inst::Cast {
                            dst,
                            kind: crate::inst::CastKind::PtrCast,
                            src: src_v,
                        });
                        return Ok((dst, ptr_ty));
                    }
                }
            }
            let (lid, root_ty) = match self.env.lookup_binding(root_name) {
                Some(Binding::Local(lid, t)) => (lid, t),
                Some(_) => {
                    return Err(LowerError::Other(format!(
                        "`&{}`: only mutable locals can be address-taken (binding is SSA / param / cell)",
                        root_name.as_str()
                    )));
                }
                None => {
                    return Err(LowerError::Other(format!(
                        "`&{}`: unbound name",
                        root_name.as_str()
                    )));
                }
            };
            let ptr_ty = MirTy::RawPtr { is_const: false, inner: Box::new(root_ty) };
            let dst = self.fb.new_value(ptr_ty.clone());
            self.fb.push_inst(Inst::AddrOfLocal { dst, local: lid });
            return Ok((dst, ptr_ty));
        }

        // Field chain: get the root's current value, then walk.
        let (mut cur_v, mut cur_ty) = match self.env.lookup_binding(root_name) {
            Some(Binding::Local(lid, t)) => {
                let v = self.fb.new_value(t.clone());
                self.fb.push_inst(Inst::UseLocal { dst: v, local: lid });
                (v, t)
            }
            Some(Binding::Ssa(v, t)) => (v, t),
            Some(Binding::Cell(cell_v, t)) => {
                // Cell-captured root: load through the cell.
                let zero = self.const_int(MirTy::I64, 0);
                let v = self.fb.new_value(t.clone());
                self.fb.push_inst(Inst::ArrayLoad { dst: v, arr: cell_v, idx: zero });
                (v, t)
            }
            None => {
                return Err(LowerError::Other(format!(
                    "`&{}...`: unbound root `{}`",
                    root_name.as_str(),
                    root_name.as_str()
                )));
            }
        };

        for (idx, fname) in fields.iter().enumerate() {
            let cid = match &cur_ty {
                MirTy::Object(cid) => *cid,
                _ => {
                    return Err(LowerError::Other(format!(
                        "`&{}...`: field `{}` accessed on non-class type `{}`",
                        root_name.as_str(),
                        fname.as_str(),
                        cur_ty,
                    )));
                }
            };
            let meta = self
                .class_meta
                .get(&cid)
                .ok_or_else(|| LowerError::Other(format!(
                    "missing class meta for cid#{} when lowering `&{}.{}`",
                    cid.0,
                    root_name.as_str(),
                    fname.as_str(),
                )))?;
            let fid = *meta.field_ix.get(fname).ok_or_else(|| LowerError::Other(format!(
                "class `{}` has no field `{}`",
                self.classes[cid.0 as usize].name.as_str(),
                fname.as_str(),
            )))?;
            let fty = meta
                .field_ty
                .get(&fid)
                .cloned()
                .ok_or_else(|| LowerError::Other("field type missing".to_string()))?;

            if idx + 1 < fields.len() {
                let next_v = self.fb.new_value(fty.clone());
                self.fb.push_inst(Inst::LoadField { dst: next_v, obj: cur_v, field: fid });
                cur_v = next_v;
                cur_ty = fty;
            } else {
                let ptr_ty = MirTy::RawPtr { is_const: false, inner: Box::new(fty) };
                let dst = self.fb.new_value(ptr_ty.clone());
                self.fb.push_inst(Inst::AddrOfField {
                    dst,
                    obj: cur_v,
                    class: cid,
                    field: fid,
                });
                return Ok((dst, ptr_ty));
            }
        }
        unreachable!("loop returns on the leaf iteration")
    }

    pub(super) fn lower_binary(
        &mut self,
        op: AstBinOp,
        lhs: &Expr,
        rhs: &Expr,
        _span: Span,
    ) -> Result<(ValueId, MirTy), LowerError> {
        let lhs_fresh = self.is_fresh_object_expr(lhs);
        let rhs_fresh = self.is_fresh_object_expr(rhs);
        let (lv0, lty0) = self.lower_expr(lhs)?;
        let (rv0, rty0) = self.lower_expr(rhs)?;
        // `@flags` enum bitwise ops: extract each operand's tag,
        // perform the op on the underlying integer repr, box the
        // result back into the same enum.
        if matches!(
            op,
            AstBinOp::BitOr | AstBinOp::BitAnd | AstBinOp::BitXor
        ) {
            if let (MirTy::Enum(le), MirTy::Enum(re)) = (&lty0, &rty0) {
                if le == re {
                    let eid = *le;
                    let layout = &self.enums[eid.0 as usize];
                    if layout.is_flags {
                        let repr_ty = layout.repr.clone();
                        let lt = self.fb.new_value(MirTy::I64);
                        self.fb.push_inst(Inst::EnumTag { dst: lt, value: lv0 });
                        let rt = self.fb.new_value(MirTy::I64);
                        self.fb.push_inst(Inst::EnumTag { dst: rt, value: rv0 });
                        let bop = match op {
                            AstBinOp::BitOr => BinOp::IOr,
                            AstBinOp::BitAnd => BinOp::IAnd,
                            AstBinOp::BitXor => BinOp::IXor,
                            _ => unreachable!(),
                        };
                        let combined = self.fb.new_value(MirTy::I64);
                        self.fb.push_inst(Inst::BinOp {
                            dst: combined,
                            op: bop,
                            lhs: lt,
                            rhs: rt,
                        });
                        // Re-box as a unit-variant enum cell; matches
                        // the runtime layout `Inst::NewEnum` produces
                        // for unit variants.
                        let dst = self.fb.new_value(MirTy::Enum(eid));
                        self.fb.push_inst(Inst::Call {
                            dst: Some(dst),
                            callee: FuncRef::Builtin(Symbol::intern("__enum_box")),
                            args: Box::new([combined]),
                        });
                        let _ = repr_ty;
                        return Ok((dst, MirTy::Enum(eid)));
                    }
                }
            }
        }
        let (lv, lty) = (lv0, lty0.clone());
        let (rv, rty) = (rv0, rty0.clone());
        // Numeric promotion (i64+f64 etc.) — pick the wider/float side.
        let (lv, rv, ty) = self.unify_numeric(lv, lty, rv, rty)?;

        let (mop, out_ty) = match op {
            AstBinOp::Add if matches!(ty, MirTy::Str) => (BinOp::StrConcat, MirTy::Str),
            AstBinOp::Eq if matches!(ty, MirTy::Str) => (BinOp::StrEq, MirTy::Bool),
            AstBinOp::Ne if matches!(ty, MirTy::Str) => (BinOp::StrNe, MirTy::Bool),
            AstBinOp::Add => (if ty.is_float() { BinOp::FAdd } else { BinOp::IAdd }, ty.clone()),
            AstBinOp::Sub => (if ty.is_float() { BinOp::FSub } else { BinOp::ISub }, ty.clone()),
            AstBinOp::Mul => (if ty.is_float() { BinOp::FMul } else { BinOp::IMul }, ty.clone()),
            AstBinOp::Div => (
                if ty.is_float() {
                    BinOp::FDiv
                } else if ty.is_signed_int() {
                    BinOp::IDivS
                } else {
                    BinOp::IDivU
                },
                ty.clone(),
            ),
            AstBinOp::Rem => (
                if ty.is_signed_int() { BinOp::IRemS } else { BinOp::IRemU },
                ty.clone(),
            ),
            AstBinOp::Eq => (if ty.is_float() { BinOp::FEq } else { BinOp::IEq }, MirTy::Bool),
            AstBinOp::Ne => (if ty.is_float() { BinOp::FNe } else { BinOp::INe }, MirTy::Bool),
            AstBinOp::Lt => (cmp_op(&ty, Cmp::Lt), MirTy::Bool),
            AstBinOp::Le => (cmp_op(&ty, Cmp::Le), MirTy::Bool),
            AstBinOp::Gt => (cmp_op(&ty, Cmp::Gt), MirTy::Bool),
            AstBinOp::Ge => (cmp_op(&ty, Cmp::Ge), MirTy::Bool),
            AstBinOp::BitAnd => (BinOp::IAnd, ty.clone()),
            AstBinOp::BitOr => (BinOp::IOr, ty.clone()),
            AstBinOp::BitXor => (BinOp::IXor, ty.clone()),
            AstBinOp::Shl => (BinOp::IShl, ty.clone()),
            AstBinOp::Shr => (
                if ty.is_signed_int() { BinOp::IShrS } else { BinOp::IShrU },
                ty.clone(),
            ),
        };
        let dst = self.fb.new_value(out_ty.clone());
        self.fb.push_inst(Inst::BinOp { dst, op: mop, lhs: lv, rhs: rv });
        // String concat consumes its operands but doesn't transfer
        // their ownership — drop any fresh +1 we got from a Call /
        // Binary / etc. so the registry-tracked buffer is freed
        // immediately. Without this, every per-frame
        // `"FPS: " + intToStr(fps)` leaks both temps for the life of
        // the process.
        if matches!(mop, BinOp::StrConcat | BinOp::StrEq | BinOp::StrNe) {
            if matches!(lty0, MirTy::Str) && lhs_fresh {
                self.fb.push_inst(Inst::Release { value: lv0 });
            }
            if matches!(rty0, MirTy::Str) && rhs_fresh {
                self.fb.push_inst(Inst::Release { value: rv0 });
            }
        }
        Ok((dst, out_ty))
    }

    pub(super) fn lower_logical(
        &mut self,
        op: LogicalOp,
        lhs: &Expr,
        rhs: &Expr,
    ) -> Result<(ValueId, MirTy), LowerError> {
        // Short-circuit via control flow:
        //   x && y  =>  if x { y } else { false }
        //   x || y  =>  if x { true } else { y }
        let cont = self.fb.new_block();
        let result = self.fb.add_block_param(cont, MirTy::Bool);

        let (lv, _) = self.lower_expr(lhs)?;
        let then_block = self.fb.new_block();
        let else_block = self.fb.new_block();
        self.fb.set_terminator(Terminator::CondBr {
            cond: lv,
            then_block,
            then_args: Box::new([]),
            else_block,
            else_args: Box::new([]),
        });

        match op {
            LogicalOp::And => {
                self.fb.switch_to(then_block);
                let (rv, _) = self.lower_expr(rhs)?;
                self.fb
                    .set_terminator(Terminator::Br { dst: cont, args: Box::new([rv]) });

                self.fb.switch_to(else_block);
                let f = self.fb.new_value(MirTy::Bool);
                self.fb.push_inst(Inst::Const { dst: f, value: MirConst::Bool(false) });
                self.fb
                    .set_terminator(Terminator::Br { dst: cont, args: Box::new([f]) });
            }
            LogicalOp::Or => {
                self.fb.switch_to(then_block);
                let t = self.fb.new_value(MirTy::Bool);
                self.fb.push_inst(Inst::Const { dst: t, value: MirConst::Bool(true) });
                self.fb
                    .set_terminator(Terminator::Br { dst: cont, args: Box::new([t]) });

                self.fb.switch_to(else_block);
                let (rv, _) = self.lower_expr(rhs)?;
                self.fb
                    .set_terminator(Terminator::Br { dst: cont, args: Box::new([rv]) });
            }
        }

        self.fb.switch_to(cont);
        Ok((result, MirTy::Bool))
}
}
