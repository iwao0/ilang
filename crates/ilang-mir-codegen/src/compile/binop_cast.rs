//! Cranelift-level lowering for `BinOp` and `CastKind`. Pure
//! function-builder transformations; no module / symbol state.

use cranelift::prelude::*;
use cranelift_codegen::ir::InstBuilder;
use cranelift_frontend::FunctionBuilder as ClifFnBuilder;

use ilang_mir::{BinOp, MirTy};

use crate::ty::mir_to_clif;

use super::CompileError;

pub(crate) fn lower_binop(fb: &mut ClifFnBuilder, op: BinOp, lhs: Value, rhs: Value) -> Value {
    // Defensive type-bridging: the MIR's `unify_numeric` aligns
    // operand MirTys but the AST→MIR path can leak a literal that
    // ended up wider than the binop's intended cell width (e.g. a
    // bare `1` inside `cellH - 1` where cellH is i32). Cranelift
    // requires both arithmetic / compare operands to share the
    // exact clif type, so widen/narrow the smaller operand on the
    // fly. For shifts we leave the count as-is (Cranelift accepts
    // any integer type for the shift amount).
    let (lhs, rhs) = match op {
        BinOp::IAdd
        | BinOp::ISub
        | BinOp::IMul
        | BinOp::IDivS
        | BinOp::IDivU
        | BinOp::IRemS
        | BinOp::IRemU
        | BinOp::IAnd
        | BinOp::IOr
        | BinOp::IXor
        | BinOp::IEq
        | BinOp::INe
        | BinOp::ILtS | BinOp::ILeS | BinOp::IGtS | BinOp::IGeS
        | BinOp::ILtU | BinOp::ILeU | BinOp::IGtU | BinOp::IGeU => {
            let lt = fb.func.dfg.value_type(lhs);
            let rt = fb.func.dfg.value_type(rhs);
            if lt != rt && lt.is_int() && rt.is_int() {
                if lt.bits() < rt.bits() {
                    (fb.ins().sextend(rt, lhs), rhs)
                } else {
                    (lhs, fb.ins().sextend(lt, rhs))
                }
            } else {
                (lhs, rhs)
            }
        }
        _ => (lhs, rhs),
    };
    match op {
        BinOp::IAdd => fb.ins().iadd(lhs, rhs),
        BinOp::ISub => fb.ins().isub(lhs, rhs),
        BinOp::IMul => fb.ins().imul(lhs, rhs),
        BinOp::IDivS => fb.ins().sdiv(lhs, rhs),
        BinOp::IDivU => fb.ins().udiv(lhs, rhs),
        BinOp::IRemS => fb.ins().srem(lhs, rhs),
        BinOp::IRemU => fb.ins().urem(lhs, rhs),
        BinOp::IShl => fb.ins().ishl(lhs, rhs),
        BinOp::IShrS => fb.ins().sshr(lhs, rhs),
        BinOp::IShrU => fb.ins().ushr(lhs, rhs),
        BinOp::IAnd => fb.ins().band(lhs, rhs),
        BinOp::IOr => fb.ins().bor(lhs, rhs),
        BinOp::IXor => fb.ins().bxor(lhs, rhs),
        BinOp::FAdd => fb.ins().fadd(lhs, rhs),
        BinOp::FSub => fb.ins().fsub(lhs, rhs),
        BinOp::FMul => fb.ins().fmul(lhs, rhs),
        BinOp::FDiv => fb.ins().fdiv(lhs, rhs),
        BinOp::IEq => fb.ins().icmp(IntCC::Equal, lhs, rhs),
        BinOp::INe => fb.ins().icmp(IntCC::NotEqual, lhs, rhs),
        BinOp::ILtS => fb.ins().icmp(IntCC::SignedLessThan, lhs, rhs),
        BinOp::ILeS => fb.ins().icmp(IntCC::SignedLessThanOrEqual, lhs, rhs),
        BinOp::IGtS => fb.ins().icmp(IntCC::SignedGreaterThan, lhs, rhs),
        BinOp::IGeS => fb.ins().icmp(IntCC::SignedGreaterThanOrEqual, lhs, rhs),
        BinOp::ILtU => fb.ins().icmp(IntCC::UnsignedLessThan, lhs, rhs),
        BinOp::ILeU => fb.ins().icmp(IntCC::UnsignedLessThanOrEqual, lhs, rhs),
        BinOp::IGtU => fb.ins().icmp(IntCC::UnsignedGreaterThan, lhs, rhs),
        BinOp::IGeU => fb.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, lhs, rhs),
        BinOp::FEq => fb.ins().fcmp(FloatCC::Equal, lhs, rhs),
        BinOp::FNe => fb.ins().fcmp(FloatCC::NotEqual, lhs, rhs),
        BinOp::FLt => fb.ins().fcmp(FloatCC::LessThan, lhs, rhs),
        BinOp::FLe => fb.ins().fcmp(FloatCC::LessThanOrEqual, lhs, rhs),
        BinOp::FGt => fb.ins().fcmp(FloatCC::GreaterThan, lhs, rhs),
        BinOp::FGe => fb.ins().fcmp(FloatCC::GreaterThanOrEqual, lhs, rhs),
        BinOp::StrEq | BinOp::StrNe | BinOp::StrConcat | BinOp::StrConcatInplace => {
            // String ops require a runtime call — wired alongside the
            // ARC runtime in a follow-up step.
            unimplemented!("string ops in mir-codegen")
        }
    }
}

pub(super) fn lower_cast(
    fb: &mut ClifFnBuilder,
    kind: ilang_mir::CastKind,
    src: Value,
    dst_ty: &MirTy,
    src_mir_ty: &MirTy,
) -> Result<Value, CompileError> {
    use ilang_mir::CastKind;
    let dst_ct = mir_to_clif(dst_ty).ok_or(CompileError::Unsupported("cast to non-clif type"))?;
    Ok(match kind {
        CastKind::IntResize | CastKind::IntSignCross => {
            let src_ty = fb.func.dfg.value_type(src);
            if src_ty.bits() == dst_ct.bits() {
                src
            } else if src_ty.bits() < dst_ct.bits() {
                // Widening: pick uextend for unsigned source (incl.
                // bool / u8 / u16 / u32 / size_t) or for explicit
                // sign-cross casts; sextend for signed widening.
                let use_unsigned = matches!(kind, CastKind::IntSignCross)
                    || src_mir_ty.is_unsigned_int();
                if use_unsigned {
                    fb.ins().uextend(dst_ct, src)
                } else {
                    fb.ins().sextend(dst_ct, src)
                }
            } else {
                fb.ins().ireduce(dst_ct, src)
            }
        }
        CastKind::IntToFloat => {
            if src_mir_ty.is_unsigned_int() {
                fb.ins().fcvt_from_uint(dst_ct, src)
            } else {
                fb.ins().fcvt_from_sint(dst_ct, src)
            }
        }
        CastKind::FloatToInt => {
            // Use the saturating variants — `fcvt_to_sint` /
            // `fcvt_to_uint` trap on out-of-range / NaN inputs,
            // which surfaces as a process SIGILL the user can't
            // catch. The `_sat` forms clamp to the destination
            // type's min/max instead, matching the semantics most
            // ilang code expects (alpha * 255 → 0..255).
            //
            // Cranelift's x64 backend only supports I32/I64 as the
            // destination for saturating float→int instructions, so
            // for narrower targets (i8/i16/u8/u16) we convert to I32
            // first. That only clamps at the I32 boundary, though — a
            // plain `ireduce` afterward takes the low bits and WRAPS
            // (300.0 as u8 → 44, 40000.0 as i16 → -25536). Re-clamp the
            // I32 result to the destination type's range in the integer
            // domain before truncating so the saturation is honoured at
            // the narrow width (300.0 as u8 → 255, 40000.0 as i16 →
            // 32767, -200.0 as i8 → -128).
            let effective_ct = if dst_ct.bits() < 32 { types::I32 } else { dst_ct };
            let converted = if dst_ty.is_unsigned_int() {
                fb.ins().fcvt_to_uint_sat(effective_ct, src)
            } else {
                fb.ins().fcvt_to_sint_sat(effective_ct, src)
            };
            if dst_ct.bits() < 32 {
                let bits = dst_ct.bits();
                let clamped = if dst_ty.is_unsigned_int() {
                    // [0, 2^bits - 1] — the unsigned convert is already
                    // ≥ 0, so an unsigned min against the max suffices.
                    let max = fb.ins().iconst(effective_ct, (1i64 << bits) - 1);
                    fb.ins().umin(converted, max)
                } else {
                    // [-2^(bits-1), 2^(bits-1) - 1]
                    let max = fb.ins().iconst(effective_ct, (1i64 << (bits - 1)) - 1);
                    let min = fb.ins().iconst(effective_ct, -(1i64 << (bits - 1)));
                    let hi = fb.ins().smin(converted, max);
                    fb.ins().smax(hi, min)
                };
                fb.ins().ireduce(dst_ct, clamped)
            } else {
                converted
            }
        }
        CastKind::FloatResize => {
            let src_ty = fb.func.dfg.value_type(src);
            if src_ty.bits() < dst_ct.bits() {
                fb.ins().fpromote(dst_ct, src)
            } else {
                fb.ins().fdemote(dst_ct, src)
            }
        }
        CastKind::StrongToWeak | CastKind::PtrCast | CastKind::PtrIntCast => {
            // Pointer reinterprets / weak conversion are identity at
            // the clif level. The REPL slot store / load path also
            // funnels float ↔ i64 round-trips through PtrIntCast; for
            // those we need a real bitcast (or a width-bridging
            // sequence so f32 can flow through an i64 slot). Other
            // mixed-width int↔int cases stay as identity to preserve
            // the legacy "same-rep reinterpret" contract every other
            // call site already depends on.
            let src_ct = fb.func.dfg.value_type(src);
            if src_ct == dst_ct {
                src
            } else if src_ct == types::I64 && dst_ct == types::F64 {
                fb.ins().bitcast(types::F64, MemFlags::new(), src)
            } else if src_ct == types::F64 && dst_ct == types::I64 {
                fb.ins().bitcast(types::I64, MemFlags::new(), src)
            } else if src_ct == types::I64 && dst_ct == types::F32 {
                let narrow = fb.ins().ireduce(types::I32, src);
                fb.ins().bitcast(types::F32, MemFlags::new(), narrow)
            } else if src_ct == types::F32 && dst_ct == types::I64 {
                let bits = fb.ins().bitcast(types::I32, MemFlags::new(), src);
                fb.ins().uextend(types::I64, bits)
            } else {
                src
            }
        }
        CastKind::OptionalWrap => {
            // `T → T?`. For heap-pointer T (object / array / etc.)
            // the bit pattern is reused (null = none). For primitives
            // we'd need to box; the lowerer treats this as identity
            // and the consumer handles unwrap explicitly.
            src
        }
    })
}
