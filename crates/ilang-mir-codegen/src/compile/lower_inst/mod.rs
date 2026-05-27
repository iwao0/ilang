//! Per-instruction MIR → cranelift lowering. The bulk of `compile/`
//! lives here: each `Inst` variant emits the cranelift sequence
//! that realises it, with the surrounding `BodyCx`-style state
//! threaded in as parameters from `lower_function`. Large variants
//! (Call / LoadField / StoreField) are split out into per-topic
//! submodules — they take the same long parameter list since
//! there isn't a dedicated context struct yet.

mod arc;
mod array;
mod call_dispatch;
mod calls;
mod enum_inst;
mod objects;
mod rtti;

use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::InstBuilder;
use cranelift_frontend::FunctionBuilder as ClifFnBuilder;
use cranelift_module::Module;

use ilang_mir::{BinOp, Inst, MirConst, MirTy, UnOp, ValueId};

use crate::ty::mir_to_clif;

use super::abi::{extend_to_i64, reduce_from_i64};
use super::binop_cast::{lower_binop, lower_cast};
use super::lower_term_const::lower_const;
use super::print_emit::emit_panic_if;
use super::print_kind::{kind_tag_of, print_kind_id, KIND_NONE};
use super::{CompileError, OBJECT_HEADER_BYTES};

/// Inline byte-wise copy of `total` bytes from `src` to `dst_addr`.
/// Mirrors the pattern used in `objects.rs` for CRepr struct copies —
/// it avoids depending on the JIT's `memcpy` libcall resolution
/// (which can race with how mir-codegen declares its own symbols).
pub(super) fn crepr_struct_copy(fb: &mut ClifFnBuilder, src: Value, dst_addr: Value, total: i64) {
    let mut copied = 0i64;
    while copied + 8 <= total {
        let v = fb.ins().load(types::I64, MemFlags::trusted(), src, copied as i32);
        fb.ins().store(MemFlags::trusted(), v, dst_addr, copied as i32);
        copied += 8;
    }
    while copied + 4 <= total {
        let v = fb.ins().load(types::I32, MemFlags::trusted(), src, copied as i32);
        fb.ins().store(MemFlags::trusted(), v, dst_addr, copied as i32);
        copied += 4;
    }
    while copied + 2 <= total {
        let v = fb.ins().load(types::I16, MemFlags::trusted(), src, copied as i32);
        fb.ins().store(MemFlags::trusted(), v, dst_addr, copied as i32);
        copied += 2;
    }
    while copied < total {
        let v = fb.ins().load(types::I8, MemFlags::trusted(), src, copied as i32);
        fb.ins().store(MemFlags::trusted(), v, dst_addr, copied as i32);
        copied += 1;
    }
}

pub(super) fn lower_inst<M: Module>(
    fb: &mut ClifFnBuilder,
    vmap: &mut HashMap<ValueId, Value>,
    module: &mut M,
    prog_ctx: &super::ProgCtx,
    fn_ctx: &super::FnCtx,
    inst: &Inst,
) -> Result<(), CompileError> {
    let super::ProgCtx {
        fn_ids,
        static_data,
        string_data,
        alloc_id,
        map_ids,
        str_ids,
        panic_aux,
        prog,
        ..
    } = *prog_ctx;
    let super::FnCtx {
        func,
        locals,
        local_slots,
        env_value,
        ..
    } = *fn_ctx;
    match inst {
        Inst::Const { dst, value } => {
            let ty = func.ty_of(*dst);
            if matches!(ty, MirTy::Unit) || matches!(value, MirConst::Unit) {
                return Ok(());
            }
            // String consts go through Cranelift `symbol_value` to get
            // the data symbol's runtime address.
            if let MirConst::Str(s) = value {
                let did = *string_data.get(s).ok_or_else(|| {
                    CompileError::Other(format!("missing string data for {:?}", s.as_str()))
                })?;
                let gv = module.declare_data_in_func(did, fb.func);
                let base = fb.ins().symbol_value(types::I64, gv);
                // The user-visible string pointer skips the 8-byte
                // length prefix (see string_data layout above).
                let off = fb.ins().iconst(types::I64, 8);
                let v = fb.ins().iadd(base, off);
                vmap.insert(*dst, v);
                return Ok(());
            }
            let cv = lower_const(fb, value, ty)?;
            vmap.insert(*dst, cv);
        }
        Inst::BinOp { dst, op, lhs, rhs } => {
            let lv = vmap[lhs];
            let rv = vmap[rhs];
            // Runtime div/0 / mod/0 check on int division.
            if matches!(
                op,
                BinOp::IDivS | BinOp::IDivU | BinOp::IRemS | BinOp::IRemU
            ) {
                let rv_ty = fb.func.dfg.value_type(rv);
                let zero = fb.ins().iconst(rv_ty, 0);
                let is_zero = fb.ins().icmp(IntCC::Equal, rv, zero);
                let msg = if matches!(op, BinOp::IRemS | BinOp::IRemU) {
                    panic_aux.msg_mod
                } else {
                    panic_aux.msg_div
                };
                emit_panic_if(fb, module, panic_aux.fn_id, msg, is_zero);
            }
            let v = match op {
                BinOp::StrConcat => {
                    let r = module.declare_func_in_func(str_ids.concat, fb.func);
                    let call = fb.ins().call(r, &[lv, rv]);
                    fb.inst_results(call)[0]
                }
                BinOp::StrConcatInplace => {
                    let r = module.declare_func_in_func(str_ids.concat_inplace, fb.func);
                    let call = fb.ins().call(r, &[lv, rv]);
                    fb.inst_results(call)[0]
                }
                BinOp::StrEq => {
                    let r = module.declare_func_in_func(str_ids.eq, fb.func);
                    let call = fb.ins().call(r, &[lv, rv]);
                    let raw = fb.inst_results(call)[0];
                    fb.ins().ireduce(types::I8, raw)
                }
                BinOp::StrNe => {
                    let r = module.declare_func_in_func(str_ids.eq, fb.func);
                    let call = fb.ins().call(r, &[lv, rv]);
                    let raw = fb.inst_results(call)[0];
                    let lo = fb.ins().ireduce(types::I8, raw);
                    let one = fb.ins().iconst(types::I8, 1);
                    fb.ins().bxor(lo, one)
                }
                _ => lower_binop(fb, *op, lv, rv),
            };
            vmap.insert(*dst, v);
        }
        Inst::UnOp { dst, op, src } => {
            let sv = vmap[src];
            let v = match op {
                UnOp::INeg => fb.ins().ineg(sv),
                UnOp::FNeg => fb.ins().fneg(sv),
                UnOp::Not => fb.ins().bnot(sv),
                UnOp::BoolNot => {
                    let zero = fb.ins().iconst(types::I8, 0);
                    fb.ins().icmp(IntCC::Equal, sv, zero)
                }
            };
            vmap.insert(*dst, v);
        }
        Inst::Cast { dst, kind, src } => {
            let sv = vmap[src];
            let dst_ty = func.ty_of(*dst);
            let src_ty = func.ty_of(*src);
            let v = lower_cast(fb, *kind, sv, dst_ty, src_ty)?;
            vmap.insert(*dst, v);
        }
        Inst::Call { dst, callee, args } => {
            calls::lower_call(fb, vmap, module, prog_ctx, fn_ctx, dst, callee, args)?;
        }
        Inst::VirtCall { .. }
        | Inst::CallIndirect { .. }
        | Inst::CallRawIndirect { .. }
        | Inst::ComCall { .. } => {
            call_dispatch::lower_call_dispatch_inst(fb, vmap, module, prog_ctx, fn_ctx, inst)?;
        }
        Inst::MakeClosure { dst, func: fid, captures } => {
            let cid = *fn_ids.get(fid).ok_or_else(|| {
                CompileError::Other(format!("missing fn id #{}", fid.0))
            })?;
            use super::layout::closure_header as ch;
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
            // Captures live at `env + 16 + idx*8`; env is the closure
            // block pointer (the trailing hidden param).
            let off = 16 + (*idx as i32) * 8;
            let raw = fb.ins().load(types::I64, MemFlags::trusted(), env_value, off);
            let dst_ty = func.ty_of(*dst).clone();
            let v = reduce_from_i64(fb, &dst_ty, raw);
            vmap.insert(*dst, v);
        }
        // ARC operations are stubbed in M1: refcount machinery
        // arrives once the runtime is wired. Treating them as no-ops
        // means programs leak heap allocations until then, which is
        Inst::Release { .. } | Inst::Retain { .. } => {
            arc::lower_arc_inst(fb, vmap, module, prog_ctx, fn_ctx, inst)?;
        }
        Inst::WeakRetain { .. }
        | Inst::WeakRelease { .. } => {}
        Inst::TypeOf { .. }
        | Inst::IsInstance { .. }
        | Inst::DowncastOrNone { .. }
        | Inst::WeakUpgrade { .. } => {
            rtti::lower_rtti_inst(fb, vmap, module, prog_ctx, fn_ctx, inst)?;
        }
        Inst::DefLocal { local, value } => {
            let v = vmap[value];
            if std::env::var("ILANG_DEBUG_DEFLOCAL").is_ok() {
                let want = func.local_tys[local.0 as usize].clone();
                let got = fb.func.dfg.value_type(v);
                eprintln!(
                    "[deflocal] fn={} local#{} declared={want} clif_val_ty={got}",
                    func.name.as_str(), local.0
                );
            }
            if let Some(slot) = local_slots[local.0 as usize] {
                fb.ins().stack_store(v, slot, 0);
            } else {
                let var = locals[local.0 as usize];
                fb.def_var(var, v);
            }
        }
        Inst::UseLocal { dst, local } => {
            let v = if let Some(slot) = local_slots[local.0 as usize] {
                let ct = mir_to_clif(&func.local_tys[local.0 as usize]).unwrap_or(types::I64);
                fb.ins().stack_load(ct, slot, 0)
            } else {
                let var = locals[local.0 as usize];
                fb.use_var(var)
            };
            vmap.insert(*dst, v);
        }
        Inst::AddrOfLocal { dst, local } => {
            let slot = local_slots[local.0 as usize]
                .expect("AddrOfLocal target local must have a StackSlot");
            let p = fb.ins().stack_addr(types::I64, slot, 0);
            vmap.insert(*dst, p);
        }
        Inst::AddrOfField { dst, obj, class, field } => {
            let obj_v = vmap[obj];
            let layout = &prog.classes[class.0 as usize];
            let offset: i64 = if matches!(
                layout.repr,
                ilang_mir::ClassRepr::CRepr
                    | ilang_mir::ClassRepr::CPacked
                    | ilang_mir::ClassRepr::CUnion
            ) {
                layout
                    .c_field_offsets
                    .get(field.0 as usize)
                    .copied()
                    .unwrap_or(0)
            } else {
                OBJECT_HEADER_BYTES as i64 + (field.0 as i64) * 8
            };
            let p = fb.ins().iadd_imm(obj_v, offset);
            vmap.insert(*dst, p);
        }
        Inst::NewObject { dst, class, init_args, init } => {
            objects::lower_new_object(fb, vmap, module, prog_ctx, fn_ctx, dst, class, init_args, init)?;
        }
        Inst::NewArray { .. }
        | Inst::NewArrayEmpty { .. }
        | Inst::NewSimd { .. }
        | Inst::ArrayLen { .. }
        | Inst::ArrayLoad { .. }
        | Inst::ArrayStore { .. } => {
            array::lower_array_inst(fb, vmap, module, prog_ctx, fn_ctx, inst)?;
        }
        Inst::NewMap { dst, key, val, entries } => {
            let new_ref = module.declare_func_in_func(map_ids.new, fb.func);
            let call = fb.ins().call(new_ref, &[]);
            let map_ptr = fb.inst_results(call)[0];
            // Tag the map's value-side runtime kind so host_map_set
            // can retain on insert and host_release_map can cascade-
            // release on drop, for any heap-typed value (Object,
            // String, Array, Tuple, Optional, Map, Closure, Enum).
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
        Inst::NewEnum { .. }
        | Inst::EnumTag { .. }
        | Inst::EnumDiscStr { .. }
        | Inst::EnumPayload { .. } => {
            enum_inst::lower_enum_inst(fb, vmap, module, prog_ctx, fn_ctx, inst)?;
        }
        Inst::NewTuple { dst, items } => {
            // Heterogeneous fixed-arity product. Hidden 16-byte
            // header lives BEFORE the user-facing pointer:
            //   base + RC      = rc
            //   base + PACKED  = packed:
            //                      bits  0-15 = arity (max 65535)
            //                      bits 16-63 = 4-bit KIND_* tag per
            //                                   element (up to 12;
            //                                   12+ leak heap content
            //                                   but the cell itself
            //                                   is still freed).
            //   base + ELEM_BASE = element 0 ← user_ptr
            // TupleExtract reads from offset 0 of user_ptr, unchanged.
            use super::layout::tuple_header as th;
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
            // packed (kinds | arity)
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
        Inst::NewOptional { dst, value } => {
            // `some(v)` → allocate a 3-cell heap [value | rc | kind_tag]
            // and return its address. `value` is at offset 0 so existing
            // unwrap / iflet paths keep reading from offset 0.
            use super::layout::optional_header as opth;
            let bytes = fb.ins().iconst(types::I64, opth::SIZE);
            let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
            let call = fb.ins().call(alloc_ref, &[bytes]);
            let ptr = fb.inst_results(call)[0];
            let v_ext = extend_to_i64(fb, vmap[value]);
            fb.ins().store(MemFlags::trusted(), v_ext, ptr, opth::VALUE);
            let one = fb.ins().iconst(types::I64, 1);
            fb.ins().store(MemFlags::trusted(), one, ptr, opth::RC);
            // Tag from the dst's static type — kind_tag mirrors the
            // Array convention: KIND_* discriminant of the inner
            // type so host_release_optional can dispatch the right
            // release fn at cascade time.
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
            let raw = fb.ins().load(types::I64, MemFlags::trusted(), p, 0);
            let dst_ty = func.ty_of(*dst).clone();
            let v = reduce_from_i64(fb, &dst_ty, raw);
            vmap.insert(*dst, v);
        }
        Inst::LoadField { dst, obj, field } => {
            objects::lower_load_field(fb, vmap, module, prog_ctx, fn_ctx, dst, obj, field)?;
        }
        Inst::StoreField { obj, field, value } => {
            objects::lower_store_field(fb, vmap, module, prog_ctx, fn_ctx, obj, field, value)?;
        }
        Inst::LoadStatic { dst, slot } => {
            let did = *static_data.get(slot).ok_or_else(|| {
                CompileError::Other(format!("missing static data slot #{}", slot.0))
            })?;
            let gv = module.declare_data_in_func(did, fb.func);
            let addr = fb
                .ins()
                .symbol_value(types::I64, gv);
            // Load type matches the slot's declared MirTy.
            let s = &prog.statics[slot.0 as usize];
            let raw = fb.ins().load(types::I64, MemFlags::trusted(), addr, 0);
            let v = match &s.ty {
                MirTy::I64 | MirTy::U64 | MirTy::Size | MirTy::SSize | MirTy::Str => raw,
                MirTy::I32 | MirTy::U32 => fb.ins().ireduce(types::I32, raw),
                MirTy::I16 | MirTy::U16 => fb.ins().ireduce(types::I16, raw),
                MirTy::I8 | MirTy::U8 | MirTy::Bool => fb.ins().ireduce(types::I8, raw),
                MirTy::F64 => fb.ins().bitcast(types::F64, MemFlags::new(), raw),
                MirTy::F32 => {
                    let r32 = fb.ins().ireduce(types::I32, raw);
                    fb.ins().bitcast(types::F32, MemFlags::new(), r32)
                }
                _ => return Err(CompileError::Unsupported("static slot type")),
            };
            vmap.insert(*dst, v);
        }
        Inst::StoreStatic { slot, value } => {
            let did = *static_data.get(slot).ok_or_else(|| {
                CompileError::Other(format!("missing static data slot #{}", slot.0))
            })?;
            let gv = module.declare_data_in_func(did, fb.func);
            let addr = fb.ins().symbol_value(types::I64, gv);
            let v = vmap[value];
            let s = &prog.statics[slot.0 as usize];
            let store_v = match &s.ty {
                MirTy::I64 | MirTy::U64 | MirTy::Size | MirTy::SSize | MirTy::Str => v,
                MirTy::I32 | MirTy::U32 | MirTy::I16 | MirTy::U16 | MirTy::I8 | MirTy::U8
                | MirTy::Bool => fb.ins().uextend(types::I64, v),
                MirTy::F64 => fb.ins().bitcast(types::I64, MemFlags::new(), v),
                MirTy::F32 => {
                    let r32 = fb.ins().bitcast(types::I32, MemFlags::new(), v);
                    fb.ins().uextend(types::I64, r32)
                }
                _ => return Err(CompileError::Unsupported("static slot store type")),
            };
            fb.ins().store(MemFlags::trusted(), store_v, addr, 0);
        }
        _ => {
            return Err(CompileError::Unsupported(
                "MIR inst kind not yet wired in mir-codegen",
            ));
        }
    }
    Ok(())
}
