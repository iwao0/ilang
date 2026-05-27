//! Optional instruction lowering — `NewOptional`, `OptionalIsSome`,
//! `OptionalUnwrap`. The `some(v)` rep is a 3-cell heap
//! `[value | rc | kind_tag]`; `none` is the zero pointer, so the
//! is_some / unwrap arms are pointer-non-null checks (with an OOB
//! panic on the unwrap fast-path).

#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::InstBuilder;
use cranelift_frontend::FunctionBuilder as ClifFnBuilder;
use cranelift_module::Module;

use ilang_mir::{Inst, MirTy, ValueId};

use super::super::abi::{extend_to_i64, reduce_from_i64};
use super::super::print_emit::emit_panic_if;
use super::super::print_kind::{kind_tag_of, KIND_NONE};
use super::super::CompileError;

pub(super) fn lower_optional_inst<M: Module>(
    fb: &mut ClifFnBuilder,
    vmap: &mut HashMap<ValueId, Value>,
    module: &mut M,
    prog_ctx: &super::super::ProgCtx,
    fn_ctx: &super::super::FnCtx,
    inst: &Inst,
) -> Result<(), CompileError> {
    let super::super::ProgCtx {
        alloc_id,
        panic_aux,
        prog,
        ..
    } = *prog_ctx;
    let super::super::FnCtx { func, .. } = *fn_ctx;
    use super::super::layout::optional_header as opth;
    match inst {
        Inst::NewOptional { dst, value } => {
            let bytes = fb.ins().iconst(types::I64, opth::SIZE);
            let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
            let call = fb.ins().call(alloc_ref, &[bytes]);
            let ptr = fb.inst_results(call)[0];
            let v_ext = extend_to_i64(fb, vmap[value]);
            fb.ins().store(MemFlags::trusted(), v_ext, ptr, opth::VALUE);
            let one = fb.ins().iconst(types::I64, 1);
            fb.ins().store(MemFlags::trusted(), one, ptr, opth::RC);
            // Tag from the dst's static type — KIND_* discriminant
            // of the inner type so host_release_optional can
            // dispatch the right release fn at cascade time.
            let dst_ty = func.ty_of(*dst).clone();
            let tag = if let MirTy::Optional(inner) = &dst_ty {
                kind_tag_of(inner, &prog.classes)
            } else {
                KIND_NONE
            };
            let tag_v = fb.ins().iconst(types::I64, tag);
            fb.ins().store(MemFlags::trusted(), tag_v, ptr, opth::KIND_TAG);
            vmap.insert(*dst, ptr);
        }
        Inst::OptionalIsSome { dst, opt } => {
            let p = vmap[opt];
            let zero = fb.ins().iconst(types::I64, 0);
            let v = fb.ins().icmp(IntCC::NotEqual, p, zero);
            vmap.insert(*dst, v);
        }
        Inst::OptionalUnwrap { dst, opt } => {
            let p = vmap[opt];
            let zero = fb.ins().iconst(types::I64, 0);
            let is_none = fb.ins().icmp(IntCC::Equal, p, zero);
            emit_panic_if(fb, module, panic_aux.fn_id, panic_aux.msg_unwrap, is_none);
            let raw = fb.ins().load(types::I64, MemFlags::trusted(), p, opth::VALUE);
            let dst_ty = func.ty_of(*dst).clone();
            let v = reduce_from_i64(fb, &dst_ty, raw);
            vmap.insert(*dst, v);
        }
        _ => unreachable!("lower_optional_inst called with non-optional inst"),
    }
    Ok(())
}
