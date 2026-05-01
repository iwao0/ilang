//! Operator lowering: unary, binary, logical short-circuit, and the
//! numeric coercion machinery (`coerce` / `widen_int` / `narrow_int`).
//! Also `emit_return` since it shares the coerce-to-target pattern.

use cranelift::prelude::*;
use cranelift_codegen::ir::types::{F32, F64, I8};
use cranelift_module::Module;
use ilang_ast::{BinOp, Expr, LogicalOp, UnOp};

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
        _ => {
            return Err(CodegenError::Unsupported {
                what: "function body produces no value".into(),
                span,
            });
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
                if signed {
                    b.ins().sdiv(lv, rv)
                } else {
                    b.ins().udiv(lv, rv)
                }
            }
            BinOp::Rem => {
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
    b.ins().jump(merge, &[then_val]);

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
    b.ins().jump(merge, &[else_val]);

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
    // Array values share runtime representation regardless of fixed-vs-
    // dynamic; allow assignment between them as long as the element type
    // matches. Need access to the kinds table — the helper passes it via
    // a separate path because `coerce` is otherwise type-table-free, so
    // we accept the "id may differ" case unconditionally for arrays
    // and trust the type checker to have caught real mismatches.
    if matches!(from, JitTy::Array(_)) && matches!(to, JitTy::Array(_)) {
        return Ok(v);
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
