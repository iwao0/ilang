//! Map instruction lowering — `NewMap`, `MapGet`, `MapSet`. The
//! map runtime is host-backed (`Box<ManagedMap>` in
//! `ilang-runtime`), so all three arms boil down to `$map.*`
//! extern calls with i64-cell payload widening.

#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::InstBuilder;
use cranelift_frontend::FunctionBuilder as ClifFnBuilder;
use cranelift_module::Module;

use ilang_mir::{Inst, ValueId};

use super::super::abi::{extend_to_i64, reduce_from_i64};
use super::super::print_kind::{kind_tag_of, print_kind_id, KIND_NONE};
use super::super::CompileError;

pub(super) fn lower_map_inst<M: Module>(
    fb: &mut ClifFnBuilder,
    vmap: &mut HashMap<ValueId, Value>,
    module: &mut M,
    prog_ctx: &super::super::ProgCtx,
    fn_ctx: &super::super::FnCtx,
    inst: &Inst,
) -> Result<(), CompileError> {
    let super::super::ProgCtx {
        map_ids,
        panic_aux,
        prog,
        ..
    } = *prog_ctx;
    let super::super::FnCtx { func, .. } = *fn_ctx;
    match inst {
        Inst::NewMap { dst, key, val, entries } => {
            let new_ref = module.declare_func_in_func(map_ids.new, fb.func);
            let call = fb.ins().call(new_ref, &[]);
            let map_ptr = fb.inst_results(call)[0];
            // Tag the map's value-side runtime kind so host_map_set
            // can retain on insert and host_release_map can cascade-
            // release on drop, for any heap-typed value.
            let val_kind = kind_tag_of(val, &prog.classes);
            if val_kind != KIND_NONE {
                let mark_ref =
                    module.declare_func_in_func(panic_aux.map_set_val_kind, fb.func);
                let kind_v = fb.ins().iconst(types::I64, val_kind);
                fb.ins().call(mark_ref, &[map_ptr, kind_v]);
            }
            // Tag the map with key/value print-kind ids so
            // `console.log(map)` can format entries correctly.
            let kk = fb.ins().iconst(types::I64, print_kind_id(key));
            let vk = fb.ins().iconst(types::I64, print_kind_id(val));
            let pk_ref =
                module.declare_func_in_func(panic_aux.map_set_print_kinds, fb.func);
            fb.ins().call(pk_ref, &[map_ptr, kk, vk]);
            let set_ref = module.declare_func_in_func(map_ids.set, fb.func);
            for (k, v) in entries.iter() {
                let kv = extend_to_i64(fb, vmap[k]);
                let vv = extend_to_i64(fb, vmap[v]);
                fb.ins().call(set_ref, &[map_ptr, kv, vv]);
            }
            vmap.insert(*dst, map_ptr);
        }
        Inst::MapGet { dst, map, key } => {
            let m = vmap[map];
            let k = extend_to_i64(fb, vmap[key]);
            let get_ref = module.declare_func_in_func(map_ids.get, fb.func);
            let call = fb.ins().call(get_ref, &[m, k]);
            let raw = fb.inst_results(call)[0];
            let dst_ty = func.ty_of(*dst).clone();
            let v = reduce_from_i64(fb, &dst_ty, raw);
            vmap.insert(*dst, v);
        }
        Inst::MapSet { map, key, value } => {
            let m = vmap[map];
            let k = extend_to_i64(fb, vmap[key]);
            let v = extend_to_i64(fb, vmap[value]);
            let set_ref = module.declare_func_in_func(map_ids.set, fb.func);
            fb.ins().call(set_ref, &[m, k, v]);
        }
        _ => unreachable!("lower_map_inst called with non-map inst"),
    }
    Ok(())
}
