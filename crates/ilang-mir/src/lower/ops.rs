//! Unary / binary / short-circuit logical operator lowering on
//! `BodyCx`.
//!
//! `lower_unary` covers `-x` / `+x` / `!x` / `~x`. `lower_binary`
//! handles arithmetic, bit-wise, shift, comparison, range and
//! `string + string` ops, calling `unify_numeric` to align operand
//! widths first. `lower_logical` builds the short-circuit `||` /
//! `&&` control flow with a fresh join block.

use ilang_ast::{BinOp as AstBinOp, Expr, LogicalOp, Span, Symbol, UnOp as AstUnOp};

use crate::inst::{BinOp, FuncRef, Inst, MirConst, Terminator, UnOp, ValueId};
use crate::types::MirTy;

use super::utils::{cmp_op, Cmp};
use super::{BodyCx, LowerError};

impl<'a> BodyCx<'a> {
    pub(super) fn lower_unary(&mut self, op: AstUnOp, e: &Expr, _span: Span) -> Result<(ValueId, MirTy), LowerError> {
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
        }
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
