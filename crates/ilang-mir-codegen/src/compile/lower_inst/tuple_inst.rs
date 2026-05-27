//! Tuple instruction lowering — `NewTuple`, `TupleExtract`. The
//! tuple cell hides a 16-byte header before the user-facing pointer:
//! `[rc | packed | e0 | e1 | ...]`, with the user ptr at the e0
//! slot. See `layout::tuple_header` for the slot offsets.

#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::InstBuilder;
use cranelift_frontend::FunctionBuilder as ClifFnBuilder;
use cranelift_module::Module;

use ilang_mir::{Inst, MirTy, ValueId};

use super::super::abi::{extend_to_i64, reduce_from_i64};
use super::super::print_kind::kind_tag_of;
use super::super::CompileError;

pub(super) fn lower_tuple_inst<M: Module>(
    fb: &mut ClifFnBuilder,
    vmap: &mut HashMap<ValueId, Value>,
    module: &mut M,
    prog_ctx: &super::super::ProgCtx,
    fn_ctx: &super::super::FnCtx,
    inst: &Inst,
) -> Result<(), CompileError> {
    let super::super::ProgCtx {
        alloc_id, prog, ..
    } = *prog_ctx;
    let super::super::FnCtx { func, .. } = *fn_ctx;
    use super::super::layout::tuple_header as th;
    match inst {
        Inst::NewTuple { dst, items } => {
            let n = items.len() as i64;
            let bytes = fb.ins().iconst(types::I64, th::ELEM_BASE as i64 + n.max(1) * 8);
            let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
            let call = fb.ins().call(alloc_ref, &[bytes]);
            let base = fb.inst_results(call)[0];
            let elem_off = fb.ins().iconst(types::I64, th::ELEM_BASE as i64);
            let ptr = fb.ins().iadd(base, elem_off);
            // rc = 1
            let one = fb.ins().iconst(types::I64, 1);
            fb.ins().store(MemFlags::trusted(), one, base, th::RC);
            // packed = arity (low 16) | per-elem KIND_* tag (high 48)
            let dst_ty = func.ty_of(*dst).clone();
            let mut packed: i64 = n & 0xFFFF;
            if let MirTy::Tuple(elems) = &dst_ty {
                for (i, ety) in elems.iter().enumerate() {
                    if i >= 12 {
                        break;
                    }
                    let kind = kind_tag_of(ety, &prog.classes) & 0xF;
                    packed |= kind << (16 + (i as i64) * 4);
                }
            }
            let mask_v = fb.ins().iconst(types::I64, packed);
            fb.ins().store(MemFlags::trusted(), mask_v, base, th::PACKED);
            for (i, it) in items.iter().enumerate() {
                let v_ext = extend_to_i64(fb, vmap[it]);
                fb.ins().store(MemFlags::trusted(), v_ext, ptr, (i as i32) * 8);
            }
            vmap.insert(*dst, ptr);
        }
        Inst::TupleExtract { dst, tup, idx } => {
            let p = vmap[tup];
            let off = (*idx as i32) * 8;
            let raw = fb.ins().load(types::I64, MemFlags::trusted(), p, off);
            let dst_ty = func.ty_of(*dst).clone();
            let v = reduce_from_i64(fb, &dst_ty, raw);
            vmap.insert(*dst, v);
        }
        _ => unreachable!("lower_tuple_inst called with non-tuple inst"),
    }
    Ok(())
}
