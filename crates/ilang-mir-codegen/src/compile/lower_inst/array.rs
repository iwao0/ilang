//! Array / SIMD instruction lowering — `NewArray`, `NewArrayEmpty`,
//! `NewSimd`, `ArrayLen`, `ArrayLoad`, `ArrayStore`. Extracted from
//! `lower_inst/mod.rs` so the dispatch match stays scannable.

#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::InstBuilder;
use cranelift_frontend::FunctionBuilder as ClifFnBuilder;
use cranelift_module::Module;

use ilang_mir::{Inst, MirTy, ValueId};

use crate::ty::mir_to_clif;

use super::super::abi::{
    crepr_struct_c_size, elem_byte_stride, elem_clif_type, extend_to_i64, ireduce_or_pass,
    reduce_from_i64,
};
use super::super::print_emit::emit_panic_if;
use super::super::print_kind::kind_tag_of;
use super::super::CompileError;
use super::crepr_struct_copy;

pub(super) fn lower_array_inst<M: Module>(
    fb: &mut ClifFnBuilder,
    vmap: &mut HashMap<ValueId, Value>,
    module: &mut M,
    prog_ctx: &super::super::ProgCtx,
    fn_ctx: &super::super::FnCtx,
    inst: &Inst,
) -> Result<(), CompileError> {
    let super::super::ProgCtx {
        alloc_id,
        prog,
        panic_aux,
        ..
    } = *prog_ctx;
    let super::super::FnCtx { func, .. } = *fn_ctx;
    match inst {
        Inst::NewArray { dst, elem, items } => {
            // Detect CRepr / CPacked / CUnion struct elements: those
            // need to be copied inline so the resulting buffer matches
            // the C layout `Elem buf[N]` byte-for-byte (rather than
            // degenerating to an array of heap pointers).
            let crepr_struct_elem_size: Option<i64> = crepr_struct_c_size(elem, &prog.classes);
            // Inline fixed-length output (when the dst MirTy carries
            // `len: Some(n)`): allocate `n*stride` bytes with no
            // header, store elements directly at `data + i*stride`.
            // This keeps the layout consistent with array fields of
            // `@extern(C)` structs that LoadField returns as inline
            // addresses.
            let dst_ty = func.ty_of(*dst).clone();
            if let MirTy::Array { len: Some(_), .. } = &dst_ty {
                let stride_bytes = crepr_struct_elem_size.unwrap_or_else(|| elem_byte_stride(elem));
                let n = items.len() as i64;
                let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
                let bytes = fb.ins().iconst(types::I64, n.max(1) * stride_bytes);
                let call = fb.ins().call(alloc_ref, &[bytes]);
                let ptr = fb.inst_results(call)[0];
                let elem_clif_opt = elem_clif_type(elem);
                for (i, it) in items.iter().enumerate() {
                    let raw = vmap[it];
                    let off = (i as i32) * (stride_bytes as i32);
                    if let Some(total) = crepr_struct_elem_size {
                        let dst_addr = if off == 0 {
                            ptr
                        } else {
                            let off_v = fb.ins().iconst(types::I64, off as i64);
                            fb.ins().iadd(ptr, off_v)
                        };
                        crepr_struct_copy(fb, raw, dst_addr, total);
                    } else if let Some(elem_ct) = elem_clif_opt {
                        let truncated = ireduce_or_pass(fb, raw, elem_ct);
                        fb.ins().store(MemFlags::trusted(), truncated, ptr, off);
                    } else {
                        let v_ext = extend_to_i64(fb, raw);
                        fb.ins().store(MemFlags::trusted(), v_ext, ptr, off);
                    }
                }
                vmap.insert(*dst, ptr);
                return Ok(());
            }
            // Layout: 6-i64 header [len | cap | data_ptr | rc | kind_tag | stride]
            // + separately-allocated `stride×capacity` buffer.
            use super::super::layout::array_header as ah;
            let stride_bytes = crepr_struct_elem_size.unwrap_or_else(|| elem_byte_stride(elem));
            let n = items.len() as i64;
            let header_bytes = fb.ins().iconst(types::I64, ah::SIZE);
            let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
            let call = fb.ins().call(alloc_ref, &[header_bytes]);
            let ptr = fb.inst_results(call)[0];
            let data_bytes = fb.ins().iconst(types::I64, n.max(1) * stride_bytes);
            let dcall = fb.ins().call(alloc_ref, &[data_bytes]);
            let data_ptr = fb.inst_results(dcall)[0];

            let len_v = fb.ins().iconst(types::I64, n);
            fb.ins().store(MemFlags::trusted(), len_v, ptr, ah::LEN);
            // Capacity equals length for a freshly built array literal.
            fb.ins().store(MemFlags::trusted(), len_v, ptr, ah::CAP);
            fb.ins().store(MemFlags::trusted(), data_ptr, ptr, ah::DATA_PTR);
            let one = fb.ins().iconst(types::I64, 1);
            fb.ins().store(MemFlags::trusted(), one, ptr, ah::RC);
            let tag = kind_tag_of(elem, &prog.classes);
            let tag_v = fb.ins().iconst(types::I64, tag);
            fb.ins().store(MemFlags::trusted(), tag_v, ptr, ah::KIND_TAG);
            let stride_v = fb.ins().iconst(types::I64, stride_bytes);
            fb.ins().store(MemFlags::trusted(), stride_v, ptr, ah::STRIDE);
            let elem_clif_opt = elem_clif_type(elem);
            for (i, it) in items.iter().enumerate() {
                let raw = vmap[it];
                let off = (i as i32) * (stride_bytes as i32);
                if let Some(total) = crepr_struct_elem_size {
                    let dst_addr = if off == 0 {
                        data_ptr
                    } else {
                        let off_v = fb.ins().iconst(types::I64, off as i64);
                        fb.ins().iadd(data_ptr, off_v)
                    };
                    crepr_struct_copy(fb, raw, dst_addr, total);
                } else if let Some(elem_ct) = elem_clif_opt {
                    let truncated = ireduce_or_pass(fb, raw, elem_ct);
                    fb.ins().store(MemFlags::trusted(), truncated, data_ptr, off);
                } else {
                    let v_ext = extend_to_i64(fb, raw);
                    fb.ins().store(MemFlags::trusted(), v_ext, data_ptr, off);
                }
            }
            vmap.insert(*dst, ptr);
        }
        Inst::NewArrayEmpty { dst, elem, fixed_len } => {
            use super::super::layout::array_header as ah;
            // CRepr / CPacked / CUnion: pack the cells inline at
            // `c_size` stride so subsequent push / index operations
            // produce a buffer C can read as `Elem buf[N]`. Falling
            // through to `elem_byte_stride` (which returns 8 for any
            // Object) builds a heap-pointer array instead, then a
            // later C-side `Elem*` deref reads garbage.
            let stride_bytes =
                crepr_struct_c_size(elem, &prog.classes).unwrap_or_else(|| elem_byte_stride(elem));
            let n = fixed_len.unwrap_or(0) as i64;
            let header_bytes = fb.ins().iconst(types::I64, ah::SIZE);
            let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
            let call = fb.ins().call(alloc_ref, &[header_bytes]);
            let ptr = fb.inst_results(call)[0];
            let cap = n.max(4);
            let data_bytes = fb.ins().iconst(types::I64, cap * stride_bytes);
            let dcall = fb.ins().call(alloc_ref, &[data_bytes]);
            let data_ptr = fb.inst_results(dcall)[0];
            let len_v = fb.ins().iconst(types::I64, n);
            let cap_v = fb.ins().iconst(types::I64, cap);
            fb.ins().store(MemFlags::trusted(), len_v, ptr, ah::LEN);
            fb.ins().store(MemFlags::trusted(), cap_v, ptr, ah::CAP);
            fb.ins().store(MemFlags::trusted(), data_ptr, ptr, ah::DATA_PTR);
            let one = fb.ins().iconst(types::I64, 1);
            fb.ins().store(MemFlags::trusted(), one, ptr, ah::RC);
            let tag = kind_tag_of(elem, &prog.classes);
            let tag_v = fb.ins().iconst(types::I64, tag);
            fb.ins().store(MemFlags::trusted(), tag_v, ptr, ah::KIND_TAG);
            let stride_v = fb.ins().iconst(types::I64, stride_bytes);
            fb.ins().store(MemFlags::trusted(), stride_v, ptr, ah::STRIDE);
            vmap.insert(*dst, ptr);
        }
        Inst::NewSimd { dst, lanes } => {
            // Pack `lanes` scalar values into a cranelift vector via
            // a temporary stack slot: store each lane at its byte
            // offset, then issue one vector load. Avoids
            // `scalar_to_vector` whose arm64 ISLE lowering is still
            // a TODO for some lane widths (e.g. `f32x2`), and keeps
            // the lowering uniform across all SIMD widths.
            let dst_ty = func.ty_of(*dst).clone();
            let cl_vec_ty = mir_to_clif(&dst_ty).ok_or(
                CompileError::Unsupported("SIMD type with no cranelift mapping"),
            )?;
            let (lane_elem, lane_count) = match &dst_ty {
                MirTy::Simd { elem, lanes: n } => (*elem, *n as i64),
                _ => return Err(CompileError::Unsupported("NewSimd on non-SIMD type")),
            };
            let lane_bytes = lane_elem.lane_bytes();
            let total = (lane_bytes * lane_count) as u32;
            let slot = fb.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                total,
                0,
            ));
            let lane_scalar_ct = elem_clif_type(&lane_elem.as_scalar_mir())
                .ok_or(CompileError::Unsupported("SIMD lane has no clif scalar type"))?;
            for (i, lane) in lanes.iter().enumerate() {
                let off = (i as i64 * lane_bytes) as i32;
                let raw = vmap[lane];
                let stored = ireduce_or_pass(fb, raw, lane_scalar_ct);
                fb.ins().stack_store(stored, slot, off);
            }
            let v = fb.ins().stack_load(cl_vec_ty, slot, 0);
            vmap.insert(*dst, v);
        }
        Inst::ArrayLen { dst, arr } => {
            use super::super::layout::array_header as ah;
            let arr_ty = func.ty_of(*arr).clone();
            let v = if let MirTy::Array { len: Some(n), .. } = &arr_ty {
                fb.ins().iconst(types::I64, *n as i64)
            } else {
                let p = vmap[arr];
                fb.ins().load(types::I64, MemFlags::trusted(), p, ah::LEN)
            };
            vmap.insert(*dst, v);
        }
        Inst::ArrayLoad { dst, arr, idx } => {
            let p = vmap[arr];
            let i_raw = vmap[idx];
            let i = extend_to_i64(fb, i_raw);
            let arr_ty = func.ty_of(*arr).clone();
            // CRepr struct cells live inline at stride `c_size`. The
            // ArrayLoad result for these is the *address* of the cell
            // (not a load), matching how a CRepr struct value is
            // already represented elsewhere — `LoadField` / call args
            // all work off that address. Applies to both fixed-length
            // (`T[N]`) and dynamic (`T[]`) arrays.
            let crepr_elem_size: Option<i64> = if let MirTy::Array { elem, .. } = &arr_ty {
                crepr_struct_c_size(elem, &prog.classes)
            } else {
                None
            };
            let inline_info = match &arr_ty {
                MirTy::Array { elem, len: Some(n) } => Some((
                    crepr_elem_size.unwrap_or_else(|| elem_byte_stride(elem)),
                    *n as i64,
                )),
                _ => None,
            };
            let (data_ptr, stride) = if let Some((s, n)) = inline_info {
                let n_v = fb.ins().iconst(types::I64, n);
                let oob_lo = fb.ins().icmp_imm(IntCC::SignedLessThan, i, 0);
                let oob_hi = fb.ins().icmp(IntCC::SignedGreaterThanOrEqual, i, n_v);
                let oob = fb.ins().bor(oob_lo, oob_hi);
                emit_panic_if(fb, module, panic_aux.fn_id, panic_aux.msg_oob, oob);
                let s_v = fb.ins().iconst(types::I64, s);
                (p, s_v)
            } else {
                use super::super::layout::array_header as ah;
                let len = fb.ins().load(types::I64, MemFlags::trusted(), p, ah::LEN);
                let oob_lo = fb.ins().icmp_imm(IntCC::SignedLessThan, i, 0);
                let oob_hi = fb.ins().icmp(IntCC::SignedGreaterThanOrEqual, i, len);
                let oob = fb.ins().bor(oob_lo, oob_hi);
                emit_panic_if(fb, module, panic_aux.fn_id, panic_aux.msg_oob, oob);
                let data_ptr = fb.ins().load(types::I64, MemFlags::trusted(), p, ah::DATA_PTR);
                let stride = fb.ins().load(types::I64, MemFlags::trusted(), p, ah::STRIDE);
                (data_ptr, stride)
            };
            let off = fb.ins().imul(i, stride);
            let addr = fb.ins().iadd(data_ptr, off);
            let dst_ty_mir = func.ty_of(*dst);
            let v = if crepr_elem_size.is_some() {
                addr
            } else {
                match elem_clif_type(dst_ty_mir) {
                    Some(elem_ct) if elem_ct == types::I8 => {
                        fb.ins().load(types::I8, MemFlags::trusted(), addr, 0)
                    }
                    Some(elem_ct) if elem_ct == types::I16 => {
                        fb.ins().load(types::I16, MemFlags::trusted(), addr, 0)
                    }
                    Some(elem_ct) if elem_ct == types::I32 => {
                        fb.ins().load(types::I32, MemFlags::trusted(), addr, 0)
                    }
                    Some(elem_ct) if elem_ct == types::F32 => {
                        fb.ins().load(types::F32, MemFlags::trusted(), addr, 0)
                    }
                    Some(elem_ct) if elem_ct == types::F64 => {
                        fb.ins().load(types::F64, MemFlags::trusted(), addr, 0)
                    }
                    _ => {
                        let raw = fb.ins().load(types::I64, MemFlags::trusted(), addr, 0);
                        reduce_from_i64(fb, dst_ty_mir, raw)
                    }
                }
            };
            vmap.insert(*dst, v);
        }
        Inst::ArrayStore { arr, idx, value } => {
            let p = vmap[arr];
            let i_raw = vmap[idx];
            let i = extend_to_i64(fb, i_raw);
            let arr_ty = func.ty_of(*arr).clone();
            // Pull the element type out so we can decide CRepr (memcpy
            // path) vs scalar (single store).
            let elem_ty = match &arr_ty {
                MirTy::Array { elem, .. } => Some((**elem).clone()),
                _ => None,
            };
            let crepr_elem_size: Option<i64> = elem_ty
                .as_ref()
                .and_then(|e| crepr_struct_c_size(e, &prog.classes));
            let inline_info = match &arr_ty {
                MirTy::Array { elem, len: Some(n) } => Some((
                    crepr_elem_size.unwrap_or_else(|| elem_byte_stride(elem)),
                    *n as i64,
                )),
                _ => None,
            };
            let (data_ptr, stride) = if let Some((s, n)) = inline_info {
                let n_v = fb.ins().iconst(types::I64, n);
                let oob_lo = fb.ins().icmp_imm(IntCC::SignedLessThan, i, 0);
                let oob_hi = fb.ins().icmp(IntCC::SignedGreaterThanOrEqual, i, n_v);
                let oob = fb.ins().bor(oob_lo, oob_hi);
                emit_panic_if(fb, module, panic_aux.fn_id, panic_aux.msg_oob, oob);
                let s_v = fb.ins().iconst(types::I64, s);
                (p, s_v)
            } else {
                use super::super::layout::array_header as ah;
                let len = fb.ins().load(types::I64, MemFlags::trusted(), p, ah::LEN);
                let oob_lo = fb.ins().icmp_imm(IntCC::SignedLessThan, i, 0);
                let oob_hi = fb.ins().icmp(IntCC::SignedGreaterThanOrEqual, i, len);
                let oob = fb.ins().bor(oob_lo, oob_hi);
                emit_panic_if(fb, module, panic_aux.fn_id, panic_aux.msg_oob, oob);
                let data_ptr = fb.ins().load(types::I64, MemFlags::trusted(), p, ah::DATA_PTR);
                let stride = fb.ins().load(types::I64, MemFlags::trusted(), p, ah::STRIDE);
                (data_ptr, stride)
            };
            let off = fb.ins().imul(i, stride);
            let addr = fb.ins().iadd(data_ptr, off);
            let raw = vmap[value];
            if let Some(total) = crepr_elem_size {
                // `raw` is the source struct's address; copy the
                // c_size bytes into the cell so the array's buffer
                // stays inline-struct-shaped.
                crepr_struct_copy(fb, raw, addr, total);
            } else {
                let val_ty_mir = func.ty_of(*value);
                match elem_clif_type(val_ty_mir) {
                    Some(elem_ct) if elem_ct != types::I64 => {
                        let truncated = ireduce_or_pass(fb, raw, elem_ct);
                        fb.ins().store(MemFlags::trusted(), truncated, addr, 0);
                    }
                    _ => {
                        let v_ext = extend_to_i64(fb, raw);
                        fb.ins().store(MemFlags::trusted(), v_ext, addr, 0);
                    }
                }
            }
        }
        _ => unreachable!("lower_array_inst called with non-array inst"),
    }
    Ok(())
}
