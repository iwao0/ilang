//! Closure-shaped instruction lowering — `MakeClosure`,
//! `FuncAddr`, `LoadCapture`. The first two emit the closure cell
//! `[fn_addr | rc | captures...]`; LoadCapture reads from inside an
//! already-bound closure (via the trailing env_value).

#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::InstBuilder;
use cranelift_frontend::FunctionBuilder as ClifFnBuilder;
use cranelift_module::Module;

use ilang_mir::{Inst, ValueId};

use super::super::abi::{extend_to_i64, reduce_from_i64};
use super::super::CompileError;

pub(super) fn lower_closure_inst<M: Module>(
    fb: &mut ClifFnBuilder,
    vmap: &mut HashMap<ValueId, Value>,
    module: &mut M,
    prog_ctx: &super::super::ProgCtx,
    fn_ctx: &super::super::FnCtx,
    inst: &Inst,
) -> Result<(), CompileError> {
    let super::super::ProgCtx {
        fn_ids, alloc_id, ..
    } = *prog_ctx;
    let super::super::FnCtx {
        func, env_value, ..
    } = *fn_ctx;
    use super::super::layout::closure_header as ch;
    match inst {
        Inst::MakeClosure { dst, func: fid, captures } => {
            let cid = *fn_ids.get(fid).ok_or_else(|| {
                CompileError::Other(format!("missing fn id #{}", fid.0))
            })?;
            let local_ref = module.declare_func_in_func(cid, fb.func);
            let n_caps = captures.len() as i64;
            let bytes = fb.ins().iconst(types::I64, (2 + n_caps) * 8);
            let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
            let call = fb.ins().call(alloc_ref, &[bytes]);
            let ptr = fb.inst_results(call)[0];
            let fn_addr = fb.ins().func_addr(types::I64, local_ref);
            fb.ins().store(MemFlags::trusted(), fn_addr, ptr, ch::FN_ADDR);
            let one = fb.ins().iconst(types::I64, 1);
            fb.ins().store(MemFlags::trusted(), one, ptr, ch::RC);
            for (i, c) in captures.iter().enumerate() {
                let v_ext = extend_to_i64(fb, vmap[c]);
                fb.ins().store(
                    MemFlags::trusted(),
                    v_ext,
                    ptr,
                    ch::CAPTURE_BASE + (i as i32) * 8,
                );
            }
            vmap.insert(*dst, ptr);
        }
        Inst::FuncAddr { dst, func: fid } => {
            // Bare 8-byte function code address — no closure box.
            // Stored as-is in `@extern(C)` struct fields of `fn(...)`
            // type so C code sees a real `T (*)(...)`.
            let cid = *fn_ids.get(fid).ok_or_else(|| {
                CompileError::Other(format!("missing fn id #{}", fid.0))
            })?;
            let local_ref = module.declare_func_in_func(cid, fb.func);
            let addr = fb.ins().func_addr(types::I64, local_ref);
            vmap.insert(*dst, addr);
        }
        Inst::LoadCapture { dst, idx } => {
            // Captures live at `env + CAPTURE_BASE + idx*8`; env is
            // the closure block pointer (the trailing hidden param).
            let off = ch::CAPTURE_BASE + (*idx as i32) * 8;
            let raw = fb.ins().load(types::I64, MemFlags::trusted(), env_value, off);
            let dst_ty = func.ty_of(*dst).clone();
            let v = reduce_from_i64(fb, &dst_ty, raw);
            vmap.insert(*dst, v);
        }
        _ => unreachable!("lower_closure_inst called with non-closure inst"),
    }
    Ok(())
}
