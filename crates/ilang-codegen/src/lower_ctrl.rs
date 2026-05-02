//! Control-flow lowering: `if` / `while` / `loop`. `break`/`continue`
//! are inlined into `lower_expr` since they're trivial jumps.

use cranelift::prelude::*;
use ilang_ast::Expr;

use crate::env::LowerCtx;
use crate::error::CodegenError;
use crate::lower_expr::lower_expr;
use crate::lower_stmt::lower_block_value;
use crate::lower_op::coerce;
use crate::ty::TV;

pub(crate) fn lower_if(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    cond: &Expr,
    then_branch: &ilang_ast::Block,
    else_branch: Option<&Expr>,
) -> Result<Option<TV>, CodegenError> {
    let then_block = b.create_block();
    let else_block = b.create_block();

    let c = lower_expr(b, lc, cond)?.ok_or_else(|| CodegenError::Unsupported {
        what: "if-cond is unit".into(),
        span: cond.span,
    })?
    .0;
    b.ins().brif(c, then_block, &[], else_block, &[]);

    b.switch_to_block(then_block);
    b.seal_block(then_block);
    let then_val = lower_block_value(b, lc, then_branch)?;

    let merge = b.create_block();
    let merge_param = match then_val {
        Some((v, _)) => Some(b.append_block_param(merge, b.func.dfg.value_type(v))),
        None => None,
    };
    if let Some((v, _)) = then_val {
        b.ins().jump(merge, &[v]);
    } else {
        b.ins().jump(merge, &[]);
    }

    b.switch_to_block(else_block);
    b.seal_block(else_block);
    let else_val = match else_branch {
        Some(e) => lower_expr(b, lc, e)?,
        None => None,
    };
    match (then_val, else_val) {
        (Some((_, tt)), Some((ev, _et))) => {
            let ev_coerced = coerce(b, (ev, _et), tt, cond.span)?;
            b.ins().jump(merge, &[ev_coerced]);
            b.switch_to_block(merge);
            b.seal_block(merge);
            Ok(merge_param.map(|p| (p, tt)))
        }
        (Some((_, tt)), None) => {
            let zero = match tt.cl() {
                Some(t) if t.is_float() => b.ins().f64const(0.0),
                Some(t) => b.ins().iconst(t, 0),
                None => unreachable!(),
            };
            b.ins().jump(merge, &[zero]);
            b.switch_to_block(merge);
            b.seal_block(merge);
            Ok(None)
        }
        (None, _) => {
            b.ins().jump(merge, &[]);
            b.switch_to_block(merge);
            b.seal_block(merge);
            Ok(None)
        }
    }
}

pub(crate) fn lower_while(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    cond: &Expr,
    body: &ilang_ast::Block,
) -> Result<(), CodegenError> {
    let header = b.create_block();
    let body_block = b.create_block();
    let after = b.create_block();

    b.ins().jump(header, &[]);
    b.switch_to_block(header);
    let c = lower_expr(b, lc, cond)?.ok_or_else(|| CodegenError::Unsupported {
        what: "while-cond is unit".into(),
        span: cond.span,
    })?
    .0;
    b.ins().brif(c, body_block, &[], after, &[]);

    b.switch_to_block(body_block);
    b.seal_block(body_block);
    lc.loops.push((header, after));
    let _ = lower_block_value(b, lc, body)?;
    lc.loops.pop();
    b.ins().jump(header, &[]);
    b.seal_block(header);

    b.switch_to_block(after);
    b.seal_block(after);
    Ok(())
}

pub(crate) fn lower_for_in(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    var: &str,
    iter: &Expr,
    body: &ilang_ast::Block,
) -> Result<(), CodegenError> {
    use cranelift_codegen::ir::types::I64;
    use crate::arc::{emit_release_heap, is_aliased_heap_source};
    use crate::runtime::{ARRAY_DATA_OFFSET, ARRAY_LEN_OFFSET};
    use crate::ty::JitTy;

    let (iter_v, iter_t) = lower_expr(b, lc, iter)?.ok_or_else(|| {
        CodegenError::Unsupported {
            what: "for-in iter is unit".into(),
            span: iter.span,
        }
    })?;
    let array_id = match iter_t {
        JitTy::Array(id) => id,
        _ => {
            return Err(CodegenError::Unsupported {
                what: "for-in expects array".into(),
                span: iter.span,
            });
        }
    };
    let elem_jty = lc.array_kinds[array_id as usize].elem;
    if !matches!(
        elem_jty,
        JitTy::I8 | JitTy::I16 | JitTy::I32 | JitTy::I64
            | JitTy::U8 | JitTy::U16 | JitTy::U32 | JitTy::U64
            | JitTy::F32 | JitTy::F64 | JitTy::Bool
    ) {
        return Err(CodegenError::Unsupported {
            what: "for-in over non-primitive element type is not yet \
                   supported in JIT"
                .into(),
            span: iter.span,
        });
    }
    let release_iter = !is_aliased_heap_source(&iter.kind);

    // Stash the array pointer in a Variable so we can read len/data
    // inside the loop blocks. Doing so also makes ARC release-after-loop
    // easy (we own the rc=1 if it was fresh).
    let arr_var = Variable::new(lc.env.next_var_id());
    b.declare_var(arr_var, I64);
    b.def_var(arr_var, iter_v);

    // Counter i.
    let i_var = Variable::new(lc.env.next_var_id());
    b.declare_var(i_var, I64);
    let zero = b.ins().iconst(I64, 0);
    b.def_var(i_var, zero);

    // Loop var x — bound for the body.
    let x_var = Variable::new(lc.env.next_var_id());
    b.declare_var(x_var, elem_jty.cl().expect("primitive elem"));
    let prev_binding = lc.env.bindings.insert(var.to_string(), (x_var, elem_jty));

    let header = b.create_block();
    let body_block = b.create_block();
    let cont = b.create_block();
    let after = b.create_block();

    b.ins().jump(header, &[]);
    b.switch_to_block(header);
    let arr = b.use_var(arr_var);
    let len = b.ins().load(I64, MemFlags::trusted(), arr, ARRAY_LEN_OFFSET);
    let i = b.use_var(i_var);
    let done = b.ins().icmp(IntCC::SignedGreaterThanOrEqual, i, len);
    b.ins().brif(done, after, &[], body_block, &[]);

    b.switch_to_block(body_block);
    b.seal_block(body_block);
    let data = b.ins().load(I64, MemFlags::trusted(), arr, ARRAY_DATA_OFFSET);
    let elem_size = b.ins().iconst(I64, elem_jty.size_bytes() as i64);
    let off = b.ins().imul(i, elem_size);
    let addr = b.ins().iadd(data, off);
    let elem = b.ins().load(
        elem_jty.cl().expect("primitive elem"),
        MemFlags::trusted(),
        addr,
        0,
    );
    b.def_var(x_var, elem);

    // `continue` must jump to `cont` (which increments) — NOT directly
    // back to `header`, otherwise i never advances and the loop spins.
    lc.loops.push((cont, after));
    let _ = lower_block_value(b, lc, body)?;
    lc.loops.pop();
    b.ins().jump(cont, &[]);

    b.switch_to_block(cont);
    b.seal_block(cont);
    let i_now = b.use_var(i_var);
    let one = b.ins().iconst(I64, 1);
    let next_i = b.ins().iadd(i_now, one);
    b.def_var(i_var, next_i);
    b.ins().jump(header, &[]);
    b.seal_block(header);

    b.switch_to_block(after);
    b.seal_block(after);

    // Restore outer binding (if any), then release the array if fresh.
    match prev_binding {
        Some(prev) => {
            lc.env.bindings.insert(var.to_string(), prev);
        }
        None => {
            lc.env.bindings.remove(var);
        }
    }
    if release_iter {
        let p = b.use_var(arr_var);
        emit_release_heap(b, lc, p, iter_t);
    }
    Ok(())
}

pub(crate) fn lower_loop(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    body: &ilang_ast::Block,
) -> Result<(), CodegenError> {
    let header = b.create_block();
    let after = b.create_block();
    b.ins().jump(header, &[]);
    b.switch_to_block(header);
    lc.loops.push((header, after));
    let _ = lower_block_value(b, lc, body)?;
    lc.loops.pop();
    b.ins().jump(header, &[]);
    b.seal_block(header);
    b.switch_to_block(after);
    b.seal_block(after);
    Ok(())
}
