//! Operator lowering: unary, binary, logical short-circuit, and the
//! numeric coercion machinery (`coerce` / `widen_int` / `narrow_int`).
//! Also `emit_return` since it shares the coerce-to-target pattern.

use cranelift::prelude::*;
use cranelift_codegen::ir::types::{F32, F64, I8};
use cranelift_module::Module;
use ilang_ast::{BinOp, Expr, ExprKind, LogicalOp, UnOp};

use crate::arc::{emit_release_string, is_aliased_heap_source};
use crate::env::LowerCtx;
use crate::error::CodegenError;
use crate::lower_expr::lower_expr;
use crate::ty::{common_numeric_ty, JitTy, TV};

pub(crate) fn emit_return(
    b: &mut FunctionBuilder,
    ret_ty: JitTy,
    body: Option<TV>,
    span: ilang_ast::Span,
) -> Result<(), CodegenError> {
    match (ret_ty, body) {
        (JitTy::Unit, _) => {
            b.ins().return_(&[]);
        }
        (_, Some((v, vt))) => {
            let v = coerce(b, (v, vt), ret_ty, span)?;
            b.ins().return_(&[v]);
        }
        // No body value, non-unit return: this only happens when an
        // earlier `return X` already terminated the live path and we're
        // emitting in a (statically) unreachable block. Cranelift still
        // needs the block to have a terminator with the right ABI, so
        // produce a zero-value of the declared type.
        (t, None) => {
            let dummy = match t.cl() {
                Some(ct) if ct.is_float() => {
                    if matches!(t, JitTy::F32) {
                        b.ins().f32const(0.0)
                    } else {
                        b.ins().f64const(0.0)
                    }
                }
                Some(ct) => b.ins().iconst(ct, 0),
                None => {
                    return Err(CodegenError::Unsupported {
                        what: "function body produces no value".into(),
                        span,
                    });
                }
            };
            b.ins().return_(&[dummy]);
        }
    }
    Ok(())
}

pub(crate) fn lower_unary(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    op: UnOp,
    expr: &Expr,
    span: ilang_ast::Span,
) -> Result<Option<TV>, CodegenError> {
    let (v, vt) = lower_expr(b, lc, expr)?.ok_or_else(|| CodegenError::Unsupported {
        what: "unary on unit".into(),
        span,
    })?;
    let r = match op {
        UnOp::Pos => v,
        UnOp::Neg => {
            if vt.is_float() {
                b.ins().fneg(v)
            } else if vt.is_signed_int() {
                b.ins().ineg(v)
            } else {
                return Err(CodegenError::Unsupported {
                    what: format!("unary `-` on {vt:?}"),
                    span,
                });
            }
        }
        UnOp::Not => {
            let one = b.ins().iconst(I8, 1);
            b.ins().bxor(v, one)
        }
        UnOp::BitNot => b.ins().bnot(v),
    };
    Ok(Some((r, vt)))
}

/// True when `e` is an integer literal (or its unary negation)
/// whose value fits the JIT integer type `t`. Mirrors the type
/// checker's `numeric_literal_fits` for binary-op operand
/// adoption — the JIT needs the same flexibility so it can cast
/// the literal-side value to the other operand's type.
fn int_literal_fits_jit(e: &Expr, t: JitTy) -> bool {
    match &e.kind {
        ExprKind::Int(n) => fits(*n, t),
        ExprKind::Unary { op: UnOp::Neg, expr: inner } => {
            if let ExprKind::Int(n) = &inner.kind {
                n.checked_neg().is_some_and(|v| fits(v, t))
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Re-emit a literal-side value at a different int width while
/// reinterpreting its signedness. Used by binary-op literal-side
/// adoption (e.g. an i64 literal `0` becoming u32 to match the
/// other operand). The bit pattern doesn't change semantically —
/// the literal already fits — so a plain narrow / widen suffices
/// (sign of the source determines the extension when widening,
/// which is harmless here because the literal fits the dest type).
fn coerce_literal_to(
    b: &mut FunctionBuilder,
    v: Value,
    from: JitTy,
    to: JitTy,
) -> Value {
    let from_w = from.int_width();
    let to_w = to.int_width();
    if to_w > from_w {
        widen_int(b, v, from_w, to, from.is_signed_int())
    } else if to_w < from_w {
        narrow_int(b, v, to_w, to)
    } else {
        v
    }
}

fn fits(n: i64, t: JitTy) -> bool {
    match t {
        JitTy::I8 => i8::try_from(n).is_ok(),
        JitTy::I16 => i16::try_from(n).is_ok(),
        JitTy::I32 => i32::try_from(n).is_ok(),
        JitTy::I64 => true,
        JitTy::U8 => u8::try_from(n).is_ok(),
        JitTy::U16 => u16::try_from(n).is_ok(),
        JitTy::U32 => u32::try_from(n).is_ok(),
        JitTy::U64 => n >= 0,
        _ => false,
    }
}

pub(crate) fn lower_binary(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    op: BinOp,
    lhs: &Expr,
    rhs: &Expr,
) -> Result<Option<TV>, CodegenError> {
    let (lv, lt) = lower_expr(b, lc, lhs)?.ok_or_else(|| CodegenError::Unsupported {
        what: "binary lhs is unit".into(),
        span: lhs.span,
    })?;
    let (rv, rt) = lower_expr(b, lc, rhs)?.ok_or_else(|| CodegenError::Unsupported {
        what: "binary rhs is unit".into(),
        span: rhs.span,
    })?;
    // String operations: `+` concatenates, `==` / `!=` go through the
    // FFI equality function. Other ops fall through to the numeric path
    // and error out.
    if matches!(lt, JitTy::Str) && matches!(rt, JitTy::Str) {
        // Operands that came from a fresh allocation (call result,
        // "a"+"b") have rc=1 and nothing else owns them, so we release
        // after use. Aliased operands (Var/Field/Index/This) stay
        // owned by their binding.
        let release_lhs = !is_aliased_heap_source(&lhs.kind);
        let release_rhs = !is_aliased_heap_source(&rhs.kind);
        match op {
            BinOp::Add => {
                let r = lc.module.declare_func_in_func(lc.strfns.concat, b.func);
                let call = b.ins().call(r, &[lv, rv]);
                let result = b.inst_results(call)[0];
                if release_lhs {
                    emit_release_string(b, lc, lv);
                }
                if release_rhs {
                    emit_release_string(b, lc, rv);
                }
                return Ok(Some((result, JitTy::Str)));
            }
            BinOp::Eq | BinOp::Ne => {
                let r = lc.module.declare_func_in_func(lc.strfns.eq, b.func);
                let call = b.ins().call(r, &[lv, rv]);
                let eq = b.inst_results(call)[0];
                if release_lhs {
                    emit_release_string(b, lc, lv);
                }
                if release_rhs {
                    emit_release_string(b, lc, rv);
                }
                let v = if matches!(op, BinOp::Eq) {
                    eq
                } else {
                    let one = b.ins().iconst(I8, 1);
                    b.ins().bxor(eq, one)
                };
                return Ok(Some((v, JitTy::Bool)));
            }
            _ => {
                return Err(CodegenError::Unsupported {
                    what: format!("operator {op:?} on strings"),
                    span: lhs.span,
                });
            }
        }
    }
    // Mirror the type checker's literal-side adoption rule: when
    // one operand is a numeric literal whose value fits the other
    // operand's int type, treat the literal as that type. Without
    // this, `u32_var != 0` (literal default i64) errors here even
    // though the type checker accepted it. Re-coerce the literal-
    // side value so its Cranelift width matches the adopted type.
    let (lv, lt, rv, rt) = if lt.is_int()
        && rt.is_int()
        && lt.is_signed_int() != rt.is_signed_int()
    {
        if int_literal_fits_jit(rhs, lt) {
            let rv = coerce_literal_to(b, rv, rt, lt);
            (lv, lt, rv, lt)
        } else if int_literal_fits_jit(lhs, rt) {
            let lv = coerce_literal_to(b, lv, lt, rt);
            (lv, rt, rv, rt)
        } else {
            (lv, lt, rv, rt)
        }
    } else {
        (lv, lt, rv, rt)
    };
    let common = common_numeric_ty(lt, rt).ok_or_else(|| CodegenError::Unsupported {
        what: format!("incompatible binary operand types {lt:?} and {rt:?}"),
        span: lhs.span,
    })?;
    let lv = coerce(b, (lv, lt), common, lhs.span)?;
    let rv = coerce(b, (rv, rt), common, rhs.span)?;
    let is_compare = matches!(
        op,
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
    );
    if is_compare {
        let v = if common.is_float() {
            let cc = match op {
                BinOp::Eq => FloatCC::Equal,
                BinOp::Ne => FloatCC::NotEqual,
                BinOp::Lt => FloatCC::LessThan,
                BinOp::Le => FloatCC::LessThanOrEqual,
                BinOp::Gt => FloatCC::GreaterThan,
                BinOp::Ge => FloatCC::GreaterThanOrEqual,
                _ => unreachable!(),
            };
            b.ins().fcmp(cc, lv, rv)
        } else {
            let signed = common.is_signed_int() || matches!(common, JitTy::Bool);
            let cc = match (op, signed) {
                (BinOp::Eq, _) => IntCC::Equal,
                (BinOp::Ne, _) => IntCC::NotEqual,
                (BinOp::Lt, true) => IntCC::SignedLessThan,
                (BinOp::Le, true) => IntCC::SignedLessThanOrEqual,
                (BinOp::Gt, true) => IntCC::SignedGreaterThan,
                (BinOp::Ge, true) => IntCC::SignedGreaterThanOrEqual,
                (BinOp::Lt, false) => IntCC::UnsignedLessThan,
                (BinOp::Le, false) => IntCC::UnsignedLessThanOrEqual,
                (BinOp::Gt, false) => IntCC::UnsignedGreaterThan,
                (BinOp::Ge, false) => IntCC::UnsignedGreaterThanOrEqual,
                _ => unreachable!(),
            };
            b.ins().icmp(cc, lv, rv)
        };
        return Ok(Some((v, JitTy::Bool)));
    }
    let v = if common.is_float() {
        match op {
            BinOp::Add => b.ins().fadd(lv, rv),
            BinOp::Sub => b.ins().fsub(lv, rv),
            BinOp::Mul => b.ins().fmul(lv, rv),
            BinOp::Div => b.ins().fdiv(lv, rv),
            BinOp::Rem => {
                return Err(CodegenError::Unsupported {
                    what: "float `%` (cranelift has no frem)".into(),
                    span: lhs.span,
                });
            }
            _ => {
                return Err(CodegenError::Unsupported {
                    what: format!("operator {op:?} on float"),
                    span: lhs.span,
                });
            }
        }
    } else {
        let signed = common.is_signed_int();
        match op {
            BinOp::Add => b.ins().iadd(lv, rv),
            BinOp::Sub => b.ins().isub(lv, rv),
            BinOp::Mul => b.ins().imul(lv, rv),
            BinOp::Div => {
                emit_div_zero_check(b, lc, rv);
                if signed {
                    b.ins().sdiv(lv, rv)
                } else {
                    b.ins().udiv(lv, rv)
                }
            }
            BinOp::Rem => {
                emit_div_zero_check(b, lc, rv);
                if signed {
                    b.ins().srem(lv, rv)
                } else {
                    b.ins().urem(lv, rv)
                }
            }
            BinOp::BitAnd => b.ins().band(lv, rv),
            BinOp::BitOr => b.ins().bor(lv, rv),
            BinOp::BitXor => b.ins().bxor(lv, rv),
            BinOp::Shl => b.ins().ishl(lv, rv),
            BinOp::Shr => {
                if signed {
                    b.ins().sshr(lv, rv)
                } else {
                    b.ins().ushr(lv, rv)
                }
            }
            _ => unreachable!("compare handled above"),
        }
    };
    Ok(Some((v, common)))
}

pub(crate) fn lower_logical(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    op: LogicalOp,
    lhs: &Expr,
    rhs: &Expr,
) -> Result<Value, CodegenError> {
    let then_block = b.create_block();
    let else_block = b.create_block();
    let merge = b.create_block();
    b.append_block_param(merge, I8);

    let l = lower_expr(b, lc, lhs)?.ok_or_else(|| CodegenError::Unsupported {
        what: "logical lhs is unit".into(),
        span: lhs.span,
    })?
    .0;
    b.ins().brif(l, then_block, &[], else_block, &[]);

    b.switch_to_block(then_block);
    b.seal_block(then_block);
    let then_val = match op {
        LogicalOp::And => lower_expr(b, lc, rhs)?
            .ok_or_else(|| CodegenError::Unsupported {
                what: "logical rhs is unit".into(),
                span: rhs.span,
            })?
            .0,
        LogicalOp::Or => b.ins().iconst(I8, 1),
    };
    b.ins().jump(merge, &[then_val.into()]);

    b.switch_to_block(else_block);
    b.seal_block(else_block);
    let else_val = match op {
        LogicalOp::And => b.ins().iconst(I8, 0),
        LogicalOp::Or => lower_expr(b, lc, rhs)?
            .ok_or_else(|| CodegenError::Unsupported {
                what: "logical rhs is unit".into(),
                span: rhs.span,
            })?
            .0,
    };
    b.ins().jump(merge, &[else_val.into()]);

    b.switch_to_block(merge);
    b.seal_block(merge);
    Ok(b.block_params(merge)[0])
}

pub(crate) fn coerce(
    b: &mut FunctionBuilder,
    (v, from): TV,
    to: JitTy,
    span: ilang_ast::Span,
) -> Result<Value, CodegenError> {
    if from == to {
        return Ok(v);
    }
    // Object → Object across class IDs: subtype upcast (Child →
    // Parent). Same wire format (i64 pointer); the typechecker
    // already verified the relation. The id mismatch is fine.
    if matches!(from, JitTy::Object(_)) && matches!(to, JitTy::Object(_)) {
        return Ok(v);
    }
    // ilang Object pointer flowing into a raw C pointer slot
    // (`*MyStruct` parameter inside @extern(C)). The Object's
    // runtime representation is already a pointer to the user
    // data area, so it just passes through as i64.
    if matches!(from, JitTy::Object(_)) && matches!(to, JitTy::I64) {
        return Ok(v);
    }
    // ilang `T[]` flowing into a raw C pointer slot (`*T` /
    // `*const T`). The array's runtime rep is a heap header whose
    // `data` field at ARRAY_DATA_OFFSET points at the contiguous
    // element buffer — that's what C wants.
    if matches!(from, JitTy::Array(_)) && matches!(to, JitTy::I64) {
        let data = b.ins().load(
            cranelift_codegen::ir::types::I64,
            MemFlags::trusted(),
            v,
            crate::runtime::ARRAY_DATA_OFFSET,
        );
        return Ok(data);
    }
    // Array values share runtime representation regardless of fixed-vs-
    // dynamic; allow assignment between them as long as the element type
    // matches. Need access to the kinds table — the helper passes it via
    // a separate path because `coerce` is otherwise type-table-free, so
    // we accept the "id may differ" case unconditionally for arrays
    // and trust the type checker to have caught real mismatches.
    if matches!(from, JitTy::Array(_)) && matches!(to, JitTy::Array(_)) {
        return Ok(v);
    }
    // Tuples share runtime representation across kinds (the offsets
    // depend only on element widths, and tuple element widths are
    // either i64 for heap pointers or the primitive's natural width;
    // the type checker has already accepted the structural shape).
    // The kind id may legitimately differ when one element is a
    // fixed-array literal and the annotation asked for dynamic, etc.
    if matches!(from, JitTy::Tuple(_)) && matches!(to, JitTy::Tuple(_)) {
        return Ok(v);
    }
    // Optional<X> values share runtime representation (i64 nullable
    // pointer) regardless of inner. Auto-wrap T → T? also lands here:
    // a heap pointer is identical to its Optional-wrapped form.
    if matches!(from, JitTy::Optional(_)) && matches!(to, JitTy::Optional(_)) {
        return Ok(v);
    }
    if from.is_heap()
        && matches!(to, JitTy::Optional(_))
    {
        return Ok(v);
    }
    // Strong → weak downgrade is bit-identical (same heap pointer).
    // The binding-side retain (which dispatches to retain_weak via
    // emit_retain_heap on JitTy::Weak) bumps the weak_rc.
    if let (JitTy::Object(a), JitTy::Weak(b)) = (from, to) {
        if a == b {
            return Ok(v);
        }
    }
    let v = match (from, to) {
        (JitTy::Bool, t) if t.is_int() => widen_int(b, v, 8, t, false),
        (t, JitTy::Bool) if t.is_int() => narrow_int(b, v, 8, t),
        (a, c) if a.is_int() && c.is_int() => {
            let from_w = a.int_width();
            let to_w = c.int_width();
            if to_w > from_w {
                widen_int(b, v, from_w, c, a.is_signed_int())
            } else if to_w < from_w {
                narrow_int(b, v, to_w, c)
            } else {
                v
            }
        }
        (a, JitTy::F32) if a.is_signed_int() => b.ins().fcvt_from_sint(F32, v),
        (a, JitTy::F32) if a.is_unsigned_int() => b.ins().fcvt_from_uint(F32, v),
        (a, JitTy::F64) if a.is_signed_int() => b.ins().fcvt_from_sint(F64, v),
        (a, JitTy::F64) if a.is_unsigned_int() => b.ins().fcvt_from_uint(F64, v),
        (JitTy::F32, JitTy::F64) => b.ins().fpromote(F64, v),
        (JitTy::F64, JitTy::F32) => b.ins().fdemote(F32, v),
        (a, c) if a.is_float() && c.is_signed_int() => {
            let cl = c.cl().expect("non-unit");
            b.ins().fcvt_to_sint_sat(cl, v)
        }
        (a, c) if a.is_float() && c.is_unsigned_int() => {
            let cl = c.cl().expect("non-unit");
            b.ins().fcvt_to_uint_sat(cl, v)
        }
        _ => {
            return Err(CodegenError::Unsupported {
                what: format!("cannot coerce {from:?} to {to:?}"),
                span,
            });
        }
    };
    Ok(v)
}

fn widen_int(
    b: &mut FunctionBuilder,
    v: Value,
    _from_width: u32,
    to: JitTy,
    signed: bool,
) -> Value {
    let to_cl = to.cl().expect("non-unit");
    if signed {
        b.ins().sextend(to_cl, v)
    } else {
        b.ins().uextend(to_cl, v)
    }
}

fn narrow_int(b: &mut FunctionBuilder, v: Value, _to_width: u32, to: JitTy) -> Value {
    let to_cl = to.cl().expect("non-unit");
    b.ins().ireduce(to_cl, v)
}

/// Emit `if rhs == 0 { panic_div_zero() }` before integer div / mod.
/// Float div by zero is intentionally NOT checked (IEEE 754 yields
/// inf / NaN, which is what users expect).
fn emit_div_zero_check(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    rv: cranelift::prelude::Value,
) {
    let ty = b.func.dfg.value_type(rv);
    let zero = b.ins().iconst(ty, 0);
    let is_zero = b.ins().icmp(IntCC::Equal, rv, zero);
    let oob = b.create_block();
    let ok = b.create_block();
    b.ins().brif(is_zero, oob, &[], ok, &[]);
    b.switch_to_block(oob);
    b.seal_block(oob);
    let r = lc.module.declare_func_in_func(lc.panic_div_zero_id, b.func);
    b.ins().call(r, &[]);
    b.ins().trap(cranelift_codegen::ir::TrapCode::user(2).expect("trap code"));
    b.switch_to_block(ok);
    b.seal_block(ok);
}
