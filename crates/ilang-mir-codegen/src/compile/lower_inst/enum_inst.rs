//! Enum-shaped instruction lowering — `NewEnum`, `EnumTag`,
//! `EnumDiscStr`, `EnumPayload`. All four poke at the enum cell's
//! `[tag | payload...]` layout (`layout::enum_header`).

#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::InstBuilder;
use cranelift_frontend::FunctionBuilder as ClifFnBuilder;
use cranelift_module::Module;

use ilang_mir::{Inst, ValueId};

use super::super::abi::{extend_to_i64, reduce_from_i64};
use super::super::print_kind::{
    kind_tag_of, KIND_ARRAY, KIND_CLOSURE, KIND_ENUM, KIND_MAP, KIND_NONE, KIND_OBJECT,
    KIND_OPTIONAL, KIND_PROMISE, KIND_STR, KIND_TUPLE,
};
use super::super::CompileError;

pub(super) fn lower_enum_inst<M: Module>(
    fb: &mut ClifFnBuilder,
    vmap: &mut HashMap<ValueId, Value>,
    module: &mut M,
    prog_ctx: &super::super::ProgCtx,
    fn_ctx: &super::super::FnCtx,
    inst: &Inst,
) -> Result<(), CompileError> {
    let super::super::ProgCtx {
        panic_aux,
        prog,
        enum_global,
        ..
    } = *prog_ctx;
    let super::super::FnCtx { func, .. } = *fn_ctx;
    use super::super::layout::enum_header as eh;
    match inst {
        Inst::NewEnum { dst, enum_id, variant, payload } => {
            let layout = &prog.enums[enum_id.0 as usize];
            let v = &layout.variants[variant.0 as usize];
            let n_payload = match &v.payload {
                ilang_mir::VariantPayload::Unit => 0i64,
                ilang_mir::VariantPayload::Tuple(ts) => ts.len() as i64,
                ilang_mir::VariantPayload::Struct(fs) => fs.len() as i64,
            };
            // Unit-variant fast path: every `EnumName.unitVariant`
            // expression is value-equivalent (just a tag), so dispatch
            // through a process-wide cache keyed by
            // (global_enum_id, discriminant). Avoids the 8-byte
            // alloc-per-call leak for hot-loop uses like
            // `gamepad.isPressed(sdl.Button.a)`.
            if n_payload == 0 {
                let global = enum_global[enum_id.0 as usize] as i64;
                let global_v = fb.ins().iconst(types::I64, global);
                let disc_v = fb.ins().iconst(types::I64, v.discriminant);
                let f = module.declare_func_in_func(panic_aux.enum_unit_get, fb.func);
                let call = fb.ins().call(f, &[global_v, disc_v]);
                let ptr = fb.inst_results(call)[0];
                vmap.insert(*dst, ptr);
                return Ok(());
            }
            // Payload variant — register with the rc-tracked enum
            // registry via __enum_alloc so the cell can be freed on
            // rc=0. Layout still `[tag | payload...]`.
            let global = enum_global[enum_id.0 as usize] as i64;
            let global_v = fb.ins().iconst(types::I64, global);
            let n_v = fb.ins().iconst(types::I64, n_payload);
            let disc_v = fb.ins().iconst(types::I64, v.discriminant);
            let alloc_fn = module.declare_func_in_func(panic_aux.enum_alloc, fb.func);
            let call = fb.ins().call(alloc_fn, &[global_v, n_v, disc_v]);
            let ptr = fb.inst_results(call)[0];
            for (i, p) in payload.iter().enumerate() {
                let v_ext = extend_to_i64(fb, vmap[p]);
                fb.ins().store(
                    MemFlags::trusted(),
                    v_ext,
                    ptr,
                    eh::PAYLOAD_BASE + (i as i32) * 8,
                );
            }
            vmap.insert(*dst, ptr);
        }
        Inst::EnumTag { dst, value } => {
            let p = vmap[value];
            let v = fb.ins().load(types::I64, MemFlags::trusted(), p, eh::TAG);
            vmap.insert(*dst, v);
        }
        Inst::EnumDiscStr { dst, enum_id, value } => {
            // `enum-as-string` cast for `: string`-repr enums.
            // Load the box's tag (variant index), then call
            // `__enum_disc_str(global, tag)` to get a fresh
            // `StringRc *` with the variant's declared
            // discriminant string.
            let p = vmap[value];
            let tag = fb.ins().load(types::I64, MemFlags::trusted(), p, eh::TAG);
            let global = enum_global[enum_id.0 as usize] as i64;
            let global_v = fb.ins().iconst(types::I64, global);
            let f = module.declare_func_in_func(panic_aux.enum_disc_str, fb.func);
            let call = fb.ins().call(f, &[global_v, tag]);
            let v = fb.inst_results(call)[0];
            vmap.insert(*dst, v);
        }
        Inst::EnumPayload { dst, value, variant: _, idx } => {
            let p = vmap[value];
            let off = eh::PAYLOAD_BASE + (*idx as i32) * 8;
            let raw = fb.ins().load(types::I64, MemFlags::trusted(), p, off);
            let dst_ty = func.ty_of(*dst).clone();
            let v = reduce_from_i64(fb, &dst_ty, raw);
            // Heap-typed payload extraction transfers ownership: the
            // extract sees the cell's stored +1 and gives the caller
            // its own +1. Pairs with `host_release_enum`'s cascade
            // on the cell's drop — without the retain, the arm-scope
            // release of the extracted binding would double-decrement.
            let kind = kind_tag_of(&dst_ty, &prog.classes);
            if kind != KIND_NONE {
                let r = match kind {
                    KIND_OBJECT => panic_aux.retain_obj,
                    KIND_ARRAY => panic_aux.retain_array,
                    KIND_OPTIONAL => panic_aux.retain_optional,
                    KIND_TUPLE => panic_aux.retain_tuple,
                    KIND_MAP => panic_aux.retain_map,
                    KIND_CLOSURE => panic_aux.retain_closure,
                    KIND_STR => panic_aux.retain_string,
                    KIND_ENUM => panic_aux.retain_enum,
                    KIND_PROMISE => panic_aux.retain_promise,
                    _ => unreachable!(),
                };
                let f = module.declare_func_in_func(r, fb.func);
                fb.ins().call(f, &[v]);
            }
            vmap.insert(*dst, v);
        }
        _ => unreachable!("lower_enum_inst called with non-enum inst"),
    }
    Ok(())
}
