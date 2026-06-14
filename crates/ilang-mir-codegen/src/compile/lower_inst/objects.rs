//! Object-shaped instruction lowering — `NewObject`, `LoadField`,
//! `StoreField`. Extracted from `lower_inst/mod.rs`.

#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::InstBuilder;
use cranelift_frontend::FunctionBuilder as ClifFnBuilder;
use cranelift_module::Module;

use ilang_mir::{MirTy, ValueId};

use super::super::abi::{
    bitfield_storage_clif_type, celem_clif_type_with_enum, chunk_max_for, elem_byte_stride,
    elem_clif_type, extend_to_i64, ireduce_or_pass, reduce_from_i64, struct_byval_size_with_max,
    struct_chunks_with_max, struct_hfa,
};
use super::super::print_kind::kind_tag_of;
use super::super::{CompileError, OBJECT_HEADER_BYTES};

pub(super) fn lower_new_object<M: Module>(
    fb: &mut ClifFnBuilder,
    vmap: &mut HashMap<ValueId, Value>,
    module: &mut M,
    prog_ctx: &super::super::ProgCtx,
    fn_ctx: &super::super::FnCtx,
    dst: &ValueId,
    class: &ilang_mir::ClassId,
    init_args: &[ValueId],
    init: &ilang_mir::FuncId,
) -> Result<(), CompileError> {
    let super::super::ProgCtx {
        fn_ids,
        alloc_id,
        prog,
        class_global,
        ..
    } = *prog_ctx;
    let super::super::FnCtx { stack_local, func, .. } = *fn_ctx;
    let layout = &prog.classes[class.0 as usize];
    // `@extern(C) struct` lives flat with no header / rc:
    // alloc exactly c_size bytes (zero-init by host_mir_alloc)
    // and bind that pointer. No init / deinit / vtable.
    if matches!(
        layout.repr,
        ilang_mir::ClassRepr::CRepr | ilang_mir::ClassRepr::CPacked | ilang_mir::ClassRepr::CUnion
    ) {
        // CRepr struct alloc. Two paths: stack-promote when
        // escape analysis cleared `dst` and the layout is FAM-free,
        // else heap.
        let stack_ok = stack_local.contains(dst) && layout.flex_elem_size == 0;
        let ptr = if stack_ok {
            let slot_size = (layout.c_size.max(1)) as u32;
            let slot = fb.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                slot_size,
                // 8-byte alignment covers every primitive a struct
                // field can hold; over-aligning is cheap for a
                // function-local slot.
                3,
            ));
            let p = fb.ins().stack_addr(types::I64, slot, 0);
            // Zero the slot to mirror `__mir_alloc`'s zero-init.
            let zero = fb.ins().iconst(types::I64, 0);
            let mut off: i32 = 0;
            while (off as i64) < slot_size as i64 {
                fb.ins().store(MemFlags::trusted(), zero, p, off);
                off += 8;
            }
            p
        } else {
            let size_v = if layout.flex_elem_size > 0 && !init_args.is_empty() {
                let n_v = vmap[&init_args[0]];
                let n_i64 = extend_to_i64(fb, n_v);
                let elem_v = fb.ins().iconst(types::I64, layout.flex_elem_size);
                let extra = fb.ins().imul(n_i64, elem_v);
                let base = fb.ins().iconst(types::I64, layout.c_size.max(0));
                fb.ins().iadd(base, extra)
            } else {
                fb.ins().iconst(types::I64, layout.c_size.max(1))
            };
            let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
            let alloc_call = fb.ins().call(alloc_ref, &[size_v]);
            fb.inst_results(alloc_call)[0]
        };
        vmap.insert(*dst, ptr);
        return Ok(());
    }
    let n_fields = layout.fields.len() as i64;
    let total_bytes = OBJECT_HEADER_BYTES as i64 + n_fields * 8;
    // Stack-promotion fast path: escape analysis cleared this `dst`,
    // so back it with a function-local Cranelift StackSlot instead
    // of __mir_alloc. Field offsets and the matching Retain/Release
    // no-ops keep behaviour consistent with the heap path.
    let ptr = if stack_local.contains(dst) {
        let slot = fb.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            total_bytes as u32,
            3,
        ));
        let p = fb.ins().stack_addr(types::I64, slot, 0);
        let zero = fb.ins().iconst(types::I64, 0);
        let mut off = 0;
        while off < total_bytes {
            fb.ins().store(MemFlags::trusted(), zero, p, off as i32);
            off += 8;
        }
        p
    } else {
        let size = fb.ins().iconst(types::I64, total_bytes);
        let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
        let alloc_call = fb.ins().call(alloc_ref, &[size]);
        fb.inst_results(alloc_call)[0]
    };
    // Store the GLOBAL class id at obj+0 — release_object,
    // host_print_object and __virt_dispatch all key off this.
    use super::super::layout::object_header as oh;
    let cid_v = fb.ins().iconst(types::I64, class_global[class.0 as usize] as i64);
    fb.ins().store(MemFlags::trusted(), cid_v, ptr, oh::CLASS_ID);
    // Stack-promoted objects use the `rc <= 0` sentinel so any
    // retain/release that slips past the codegen-side `stack_local`
    // check (e.g. a `Release` of a `use_local` re-read of the
    // promoted SSA, where the re-read isn't itself in the set)
    // becomes a runtime no-op — `atomic_retain`/`atomic_release`
    // skip non-positive rc slots — instead of trying to free a
    // stack address.
    let rc_init: i64 = if stack_local.contains(dst) { -1 } else { 1 };
    let rc_v = fb.ins().iconst(types::I64, rc_init);
    fb.ins().store(MemFlags::trusted(), rc_v, ptr, oh::RC);

    // Dynamic-array fields default to an empty array (syntax.md §7
    // "Field defaults"). `__mir_alloc` only zeroed the slot, so without
    // this the field reads back as a NULL pointer and the first
    // `.length` / `.push` call dereferences zero.
    //
    // `__c_array_to_array(0, 0, stride, kind_tag)` produces a fresh
    // 48 B header / 0-length buffer matching the layout a non-default
    // array literal would. Any `init` that subsequently overwrites the
    // field falls through `StoreField`, which releases this placeholder
    // before installing the user's value (same path that array
    // re-assignments already use).
    let c_array_id = prog_ctx
        .builtin_ids
        .get("c_array_to_array")
        .map(|(id, _)| *id);
    if let Some(c_array_id) = c_array_id {
        let c_array_ref = module.declare_func_in_func(c_array_id, fb.func);
        for (fi, f) in layout.fields.iter().enumerate() {
            if let MirTy::Array { elem, len: None } = &f.ty {
                let stride = elem_byte_stride(elem);
                let kind_tag = kind_tag_of(elem, &prog.classes);
                let zero = fb.ins().iconst(types::I64, 0);
                let stride_v = fb.ins().iconst(types::I64, stride);
                let kind_v = fb.ins().iconst(types::I64, kind_tag);
                let call =
                    fb.ins().call(c_array_ref, &[zero, zero, stride_v, kind_v]);
                let arr_ptr = fb.inst_results(call)[0];
                let off = OBJECT_HEADER_BYTES + (fi as i32) * 8;
                fb.ins().store(MemFlags::trusted(), arr_ptr, ptr, off);
            }
        }
    }

    if init.0 != u32::MAX {
        let cid = *fn_ids.get(init).ok_or_else(|| {
            CompileError::Other(format!("missing init fn id #{}", init.0))
        })?;
        let local_ref = module.declare_func_in_func(cid, fb.func);
        let mut args: Vec<Value> = Vec::with_capacity(init_args.len() + 2);
        args.push(ptr);
        // Match `lower_call`'s CRepr by-value expansion so the
        // declared init signature (HFA-spread / i64-chunked CRepr
        // params built by `clif_signature_for`) lines up with the
        // call site. Without this, a `Quad { r,g,b,a: f64 }` init
        // param expands to 4 f64 AbiParams on the callee side while
        // the caller pushed a single i64 pointer, blowing up the
        // verifier with "got 3, expected 6" / "arg has type i64,
        // expected f64".
        let init_func = &prog.functions[init.0 as usize];
        let callee_chunk_max = chunk_max_for(init_func);
        let hfa_ok = fb.func.signature.call_conv
            != cranelift_codegen::isa::CallConv::WindowsFastcall;
        for a in init_args.iter() {
            let av = vmap[a];
            let aty = func.ty_of(*a);
            let mut expanded = false;
            if hfa_ok {
                if let Some((elem_ct, count)) = struct_hfa(aty, prog) {
                    let elem_size: i32 = if elem_ct == types::F32 { 4 } else { 8 };
                    for c in 0..count {
                        let v = fb.ins().load(
                            elem_ct,
                            MemFlags::trusted(),
                            av,
                            (c as i32) * elem_size,
                        );
                        args.push(v);
                    }
                    expanded = true;
                }
            }
            if !expanded {
                if let Some(chunks) = struct_chunks_with_max(aty, prog, callee_chunk_max) {
                    for c in 0..chunks {
                        let cell = fb.ins().load(
                            types::I64,
                            MemFlags::trusted(),
                            av,
                            (c as i32) * 8,
                        );
                        args.push(cell);
                    }
                    expanded = true;
                }
            }
            if !expanded {
                if let Some(size) =
                    struct_byval_size_with_max(aty, prog, callee_chunk_max)
                {
                    let slot = fb.create_sized_stack_slot(
                        cranelift_codegen::ir::StackSlotData::new(
                            cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                            size as u32,
                            3,
                        ),
                    );
                    let copy_ptr = fb.ins().stack_addr(types::I64, slot, 0);
                    let mut off: i32 = 0;
                    while (off as i64) < size {
                        let cell =
                            fb.ins().load(types::I64, MemFlags::trusted(), av, off);
                        fb.ins().store(MemFlags::trusted(), cell, copy_ptr, off);
                        off += 8;
                    }
                    args.push(copy_ptr);
                    expanded = true;
                }
            }
            if !expanded {
                args.push(av);
            }
        }
        // Trailing env-ptr (unused by init).
        let zero = fb.ins().iconst(types::I64, 0);
        args.push(zero);
        let call_inst = fb.ins().call(local_ref, &args);
        // init returns `this`; use it (in case the runtime ever
        // wraps the receiver).
        let returned = fb.inst_results(call_inst).first().copied();
        let result = returned.unwrap_or(ptr);
        vmap.insert(*dst, result);
    } else {
        vmap.insert(*dst, ptr);
    }
    Ok(())
}

pub(super) fn lower_load_field<M: Module>(
    fb: &mut ClifFnBuilder,
    vmap: &mut HashMap<ValueId, Value>,
    module: &mut M,
    prog_ctx: &super::super::ProgCtx,
    fn_ctx: &super::super::FnCtx,
    dst: &ValueId,
    obj: &ValueId,
    field: &ilang_mir::FieldId,
) -> Result<(), CompileError> {
    let super::super::ProgCtx {
        str_ids,
        panic_aux,
        prog,
        enum_global,
        ..
    } = *prog_ctx;
    let super::super::FnCtx { func, .. } = *fn_ctx;
    let obj_v = vmap[obj];
    let dst_ty_mir = func.ty_of(*dst).clone();
    let obj_ty_mir = func.ty_of(*obj).clone();
    let (crepr, bit_info) = if let MirTy::Object(cid) = &obj_ty_mir {
        let layout = &prog.classes[cid.0 as usize];
        if matches!(
            layout.repr,
            ilang_mir::ClassRepr::CRepr
                | ilang_mir::ClassRepr::CPacked
                | ilang_mir::ClassRepr::CUnion
        ) {
            let off = layout.c_field_offsets.get(field.0 as usize).copied().unwrap_or(0);
            let bf = layout
                .fields
                .get(field.0 as usize)
                .and_then(|f| f.bit_field);
            (Some(off), bf)
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };
    // Bitfield read: load the storage unit, shift right by
    // bit_offset, mask off the high bits beyond `width`.
    if let (Some(c_off), Some(bf)) = (crepr, bit_info) {
        let storage_ct = bitfield_storage_clif_type(&dst_ty_mir);
        let raw = fb.ins().load(
            storage_ct,
            MemFlags::trusted(),
            obj_v,
            c_off as i32,
        );
        let shifted = if bf.offset == 0 {
            raw
        } else {
            let shift = fb.ins().iconst(storage_ct, bf.offset as i64);
            fb.ins().ushr(raw, shift)
        };
        let mask_val: u64 = if bf.width >= 64 {
            u64::MAX
        } else {
            (1u64 << bf.width) - 1
        };
        let mask = fb.ins().iconst(storage_ct, mask_val as i64);
        let v = fb.ins().band(shifted, mask);
        vmap.insert(*dst, v);
        return Ok(());
    }
    // FAM (C99 flexible array member) — last field of a CRepr
    // struct typed `T[]` (no len). The field has no slot of
    // its own; its data starts at obj_v + c_off and runs to
    // the end of the over-allocated buffer. We don't know the
    // element count statically (caller maintains it in a
    // sibling field), so wrap the inline area in a synthetic
    // dyn-array header with len=i64::MAX so subsequent
    // ArrayLoad / ArrayStore bounds checks become no-ops, but
    // the data pointer aliases the inline buffer so reads
    // and writes hit the real storage.
    if let Some(c_off) = crepr {
        let is_fam = matches!(&dst_ty_mir, MirTy::Array { len: None, .. })
            && matches!(
                &obj_ty_mir,
                MirTy::Object(_cid)
            );
        if is_fam {
            if let MirTy::Object(cid) = &obj_ty_mir {
                let layout = &prog.classes[cid.0 as usize];
                let last_ix = layout.fields.len().saturating_sub(1);
                if field.0 as usize == last_ix && layout.flex_elem_size > 0 {
                    let elem = if let MirTy::Array { elem, .. } = &dst_ty_mir {
                        (**elem).clone()
                    } else {
                        MirTy::I64
                    };
                    let stride = layout.flex_elem_size;
                    let kind_tag = if matches!(elem, MirTy::Object(_)) {
                        1
                    } else {
                        0
                    };
                    let off_v = fb.ins().iconst(types::I64, c_off);
                    let inline_ptr = fb.ins().iadd(obj_v, off_v);
                    let len_v = fb.ins().iconst(types::I64, i64::MAX);
                    let stride_v = fb.ins().iconst(types::I64, stride);
                    let kind_v = fb.ins().iconst(types::I64, kind_tag);
                    let f = module.declare_func_in_func(str_ids.fixed_to_dyn, fb.func);
                    let call = fb.ins().call(f, &[inline_ptr, len_v, stride_v, kind_v]);
                    let v = fb.inst_results(call)[0];
                    vmap.insert(*dst, v);
                    return Ok(());
                }
            }
        }
        // Unit-only enum field: read the discriminant at the
        // repr's natural width, then look up the cached unit
        // cell so downstream `EnumTag` / `match` see a
        // proper heap-box pointer. The lookup aborts if the
        // value the C side wrote isn't a declared variant —
        // matches the `repr(C)` panic-on-unknown contract
        // discussed in the language design notes. `@flags`
        // enums are the exception: any OR-combined bit pattern
        // of the underlying integer is a valid value, so the
        // check is skipped and the unchecked unit-cell lookup
        // is used directly.
        //
        // The inline-slot kind is now a static MirTy probe — the
        // field's metadata carries `CReprEnum(eid)` whenever
        // `decl/extern_c.rs::is_inline_repr_enum` would be true,
        // so we no longer have to recompute the unit-only /
        // int-repr check at every codegen call.
        let field_meta_ty = if let MirTy::Object(cid) = &obj_ty_mir {
            prog.classes[cid.0 as usize]
                .fields
                .get(field.0 as usize)
                .map(|fd| fd.ty.clone())
        } else {
            None
        };
        if let Some(MirTy::CReprEnum(eid)) = field_meta_ty {
            let layout = &prog.enums[eid.0 as usize];
            let repr_ct = elem_clif_type(&layout.repr).unwrap_or(types::I64);
            let raw = fb.ins().load(repr_ct, MemFlags::trusted(), obj_v, c_off as i32);
            let disc_i64 = if repr_ct == types::I64 {
                raw
            } else if layout.repr.is_signed_int() {
                fb.ins().sextend(types::I64, raw)
            } else {
                fb.ins().uextend(types::I64, raw)
            };
            let global = enum_global[eid.0 as usize] as i64;
            let global_v = fb.ins().iconst(types::I64, global);
            let helper = if layout.is_flags {
                panic_aux.enum_unit_get
            } else {
                panic_aux.enum_unit_get_checked
            };
            let f = module.declare_func_in_func(helper, fb.func);
            let call = fb.ins().call(f, &[global_v, disc_i64]);
            let v = fb.inst_results(call)[0];
            vmap.insert(*dst, v);
            return Ok(());
        }
        // CRepr: load with the field's natural type at the
        // computed byte offset. Nested CRepr struct fields
        // return the inline address.
        let v = match elem_clif_type(&dst_ty_mir) {
            Some(elem_ct) if elem_ct == types::I8 => {
                fb.ins().load(types::I8, MemFlags::trusted(), obj_v, c_off as i32)
            }
            Some(elem_ct) if elem_ct == types::I16 => {
                fb.ins().load(types::I16, MemFlags::trusted(), obj_v, c_off as i32)
            }
            Some(elem_ct) if elem_ct == types::I32 => {
                fb.ins().load(types::I32, MemFlags::trusted(), obj_v, c_off as i32)
            }
            Some(elem_ct) if elem_ct == types::F32 => {
                fb.ins().load(types::F32, MemFlags::trusted(), obj_v, c_off as i32)
            }
            Some(elem_ct) if elem_ct == types::F64 => {
                fb.ins().load(types::F64, MemFlags::trusted(), obj_v, c_off as i32)
            }
            _ => {
                // Nested CRepr struct, fixed-size array, or
                // i64-sized field — produce the inline address
                // (additive offset) for composites, otherwise
                // load the i64 cell.
                let returns_inline = match &dst_ty_mir {
                    MirTy::Object(inner_cid) => {
                        let inner = &prog.classes[inner_cid.0 as usize];
                        !inner.is_handle
                            && matches!(
                                inner.repr,
                                ilang_mir::ClassRepr::CRepr
                                    | ilang_mir::ClassRepr::CPacked
                                    | ilang_mir::ClassRepr::CUnion
                            )
                    }
                    MirTy::Array { len: Some(_), .. } => true,
                    _ => false,
                };
                if returns_inline {
                    let off_v = fb.ins().iconst(types::I64, c_off);
                    fb.ins().iadd(obj_v, off_v)
                } else {
                    fb.ins().load(
                        types::I64,
                        MemFlags::trusted(),
                        obj_v,
                        c_off as i32,
                    )
                }
            }
        };
        vmap.insert(*dst, v);
    } else {
        let off = OBJECT_HEADER_BYTES + (field.0 as i32) * 8;
        let raw = fb.ins().load(types::I64, MemFlags::trusted(), obj_v, off);
        let v = reduce_from_i64(fb, &dst_ty_mir, raw);
        vmap.insert(*dst, v);
    }
    Ok(())
}

pub(super) fn lower_store_field<M: Module>(
    fb: &mut ClifFnBuilder,
    vmap: &mut HashMap<ValueId, Value>,
    _module: &mut M,
    prog_ctx: &super::super::ProgCtx,
    fn_ctx: &super::super::FnCtx,
    obj: &ValueId,
    field: &ilang_mir::FieldId,
    value: &ValueId,
) -> Result<(), CompileError> {
    let super::super::ProgCtx { prog, .. } = *prog_ctx;
    let super::super::FnCtx { func, .. } = *fn_ctx;
    let obj_v = vmap[obj];
    let obj_ty_mir = func.ty_of(*obj).clone();
    let (crepr, bit_info) = if let MirTy::Object(cid) = &obj_ty_mir {
        let layout = &prog.classes[cid.0 as usize];
        if matches!(
            layout.repr,
            ilang_mir::ClassRepr::CRepr
                | ilang_mir::ClassRepr::CPacked
                | ilang_mir::ClassRepr::CUnion
        ) {
            let off = layout.c_field_offsets.get(field.0 as usize).copied().unwrap_or(0);
            let bf = layout
                .fields
                .get(field.0 as usize)
                .and_then(|f| f.bit_field);
            (Some(off), bf)
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };
    // Bitfield write: read-modify-write: load storage, mask
    // off the field's bits, OR in the new value's bits at
    // the right offset, store back.
    if let (Some(c_off), Some(bf)) = (crepr, bit_info) {
        let raw_val = vmap[value];
        // The storage unit width is driven by the FIELD's declared
        // type (matching the read path), not the assigned value's —
        // otherwise a narrower RHS would read-modify-write the wrong
        // number of bytes.
        let field_ty = if let MirTy::Object(cid) = &obj_ty_mir {
            prog.classes[cid.0 as usize].fields[field.0 as usize].ty.clone()
        } else {
            func.ty_of(*value).clone()
        };
        let storage_ct = bitfield_storage_clif_type(&field_ty);
        let cur = fb.ins().load(
            storage_ct,
            MemFlags::trusted(),
            obj_v,
            c_off as i32,
        );
        let mask_val: u64 = if bf.width >= 64 {
            u64::MAX
        } else {
            (1u64 << bf.width) - 1
        };
        let inv_mask_val = !(mask_val << bf.offset);
        let inv_mask = fb.ins().iconst(storage_ct, inv_mask_val as i64);
        let cleared = fb.ins().band(cur, inv_mask);
        let v_truncated = ireduce_or_pass(fb, raw_val, storage_ct);
        let mask = fb.ins().iconst(storage_ct, mask_val as i64);
        let v_masked = fb.ins().band(v_truncated, mask);
        let v_shifted = if bf.offset == 0 {
            v_masked
        } else {
            let shift = fb.ins().iconst(storage_ct, bf.offset as i64);
            fb.ins().ishl(v_masked, shift)
        };
        let new_val = fb.ins().bor(cleared, v_shifted);
        fb.ins().store(MemFlags::trusted(), new_val, obj_v, c_off as i32);
        return Ok(());
    }
    if let Some(c_off) = crepr {
        let val_ty_mir = func.ty_of(*value).clone();
        let raw = vmap[value];
        // If the field type is itself a CRepr struct, copy
        // the source struct's bytes into the destination's
        // inline region rather than storing the pointer.
        if let MirTy::Object(inner_cid) = &val_ty_mir {
            let inner_layout = &prog.classes[inner_cid.0 as usize];
            if !inner_layout.is_handle
                && matches!(
                    inner_layout.repr,
                    ilang_mir::ClassRepr::CRepr
                        | ilang_mir::ClassRepr::CPacked
                        | ilang_mir::ClassRepr::CUnion
                )
            {
                let dst_addr = if c_off == 0 {
                    obj_v
                } else {
                    let off_v = fb.ins().iconst(types::I64, c_off);
                    fb.ins().iadd(obj_v, off_v)
                };
                // Inline byte-wise copy of `c_size` bytes —
                // avoids depending on the JIT's memcpy libcall
                // resolution, which can race with how mir-codegen
                // declares its own symbols.
                let total = inner_layout.c_size.max(0);
                let mut copied = 0i64;
                while copied + 8 <= total {
                    let v = fb.ins().load(
                        types::I64,
                        MemFlags::trusted(),
                        raw,
                        copied as i32,
                    );
                    fb.ins().store(
                        MemFlags::trusted(),
                        v,
                        dst_addr,
                        copied as i32,
                    );
                    copied += 8;
                }
                while copied + 4 <= total {
                    let v = fb.ins().load(
                        types::I32,
                        MemFlags::trusted(),
                        raw,
                        copied as i32,
                    );
                    fb.ins().store(
                        MemFlags::trusted(),
                        v,
                        dst_addr,
                        copied as i32,
                    );
                    copied += 4;
                }
                while copied + 2 <= total {
                    let v = fb.ins().load(
                        types::I16,
                        MemFlags::trusted(),
                        raw,
                        copied as i32,
                    );
                    fb.ins().store(
                        MemFlags::trusted(),
                        v,
                        dst_addr,
                        copied as i32,
                    );
                    copied += 2;
                }
                while copied < total {
                    let v = fb.ins().load(
                        types::I8,
                        MemFlags::trusted(),
                        raw,
                        copied as i32,
                    );
                    fb.ins().store(
                        MemFlags::trusted(),
                        v,
                        dst_addr,
                        copied as i32,
                    );
                    copied += 1;
                }
                return Ok(());
            }
        }
        // Fixed-length array field (`pos: f32[3]` etc.) — the
        // source SSA value is the base pointer over the array's
        // inline data (header-less; `lower_array_literal_with_hint`
        // returned this for `MirTy::Array { len: Some(_), .. }`).
        // Copy the static `len * elem_stride` bytes into the
        // field's embedded slot rather than storing the pointer as
        // an i64. Element strides we know how to handle here are
        // the scalar widths `elem_byte_stride` covers — nested
        // CRepr-struct elements would need the per-class `c_size`
        // and aren't exercised yet, so leave that for the future
        // and assert against it.
        if let MirTy::Array { elem, len: Some(n) } = &val_ty_mir {
            let stride = elem_byte_stride(elem);
            let total = (*n as i64) * stride;
            let dst_addr = if c_off == 0 {
                obj_v
            } else {
                let off_v = fb.ins().iconst(types::I64, c_off);
                fb.ins().iadd(obj_v, off_v)
            };
            let mut copied = 0i64;
            while copied + 8 <= total {
                let v = fb.ins().load(
                    types::I64,
                    MemFlags::trusted(),
                    raw,
                    copied as i32,
                );
                fb.ins().store(
                    MemFlags::trusted(),
                    v,
                    dst_addr,
                    copied as i32,
                );
                copied += 8;
            }
            while copied + 4 <= total {
                let v = fb.ins().load(
                    types::I32,
                    MemFlags::trusted(),
                    raw,
                    copied as i32,
                );
                fb.ins().store(
                    MemFlags::trusted(),
                    v,
                    dst_addr,
                    copied as i32,
                );
                copied += 4;
            }
            while copied + 2 <= total {
                let v = fb.ins().load(
                    types::I16,
                    MemFlags::trusted(),
                    raw,
                    copied as i32,
                );
                fb.ins().store(
                    MemFlags::trusted(),
                    v,
                    dst_addr,
                    copied as i32,
                );
                copied += 2;
            }
            while copied < total {
                let v = fb.ins().load(
                    types::I8,
                    MemFlags::trusted(),
                    raw,
                    copied as i32,
                );
                fb.ins().store(
                    MemFlags::trusted(),
                    v,
                    dst_addr,
                    copied as i32,
                );
                copied += 1;
            }
            return Ok(());
        }
        // Unit-only enum field: the SSA value is a heap-box
        // pointer; the C struct slot wants the underlying
        // discriminant. Load tag from the box (offset 0) and
        // narrow to the field's repr width before storing.
        //
        // Uses the static `field_meta_ty == CReprEnum` probe
        // instead of the historical `val_ty_mir + unit_only`
        // dynamic check — `decl/extern_c.rs` already decided
        // the inline layout at field declaration time.
        let field_meta_ty = if let MirTy::Object(cid) = &obj_ty_mir {
            prog.classes[cid.0 as usize]
                .fields
                .get(field.0 as usize)
                .map(|fd| fd.ty.clone())
        } else {
            None
        };
        if let Some(MirTy::CReprEnum(eid)) = &field_meta_ty {
            let layout = &prog.enums[eid.0 as usize];
            let repr_ct = elem_clif_type(&layout.repr).unwrap_or(types::I64);
            let disc = fb.ins().load(types::I64, MemFlags::trusted(), raw, 0);
            let truncated = ireduce_or_pass(fb, disc, repr_ct);
            fb.ins().store(MemFlags::trusted(), truncated, obj_v, c_off as i32);
            return Ok(());
        }
        // Decide the store width from the FIELD's declared type, not
        // the value's. A CRepr struct field declared `i32` must be
        // written with a 4-byte store even when the rhs is an i64
        // const (Rust-side `iconst.i64 99` flowing into `seq: i32 = 99`).
        // Using the value type let an i64 const through unchanged,
        // emitting an 8-byte store at `c_off`, which overran the field
        // by 4 bytes and corrupted whatever sat in the next 4 bytes —
        // for `Slot[]` data buffer (16 byte = 2 × 8) the overrun
        // landed in the heap's tail-guard region, which decayed to
        // SIGABRT under `__mir_free`'s libmalloc consistency checks
        // ~8% of the time (race confirmed by `ILANG_HEAP_GUARD=1`).
        let store_ty_mir = field_meta_ty.as_ref().unwrap_or(&val_ty_mir);
        match celem_clif_type_with_enum(prog, store_ty_mir) {
            Some(elem_ct) if elem_ct != types::I64 => {
                let truncated = ireduce_or_pass(fb, raw, elem_ct);
                fb.ins().store(MemFlags::trusted(), truncated, obj_v, c_off as i32);
            }
            _ => {
                let v_ext = extend_to_i64(fb, raw);
                fb.ins().store(MemFlags::trusted(), v_ext, obj_v, c_off as i32);
            }
        }
    } else {
        let off = OBJECT_HEADER_BYTES + (field.0 as i32) * 8;
        let store_v = extend_to_i64(fb, vmap[value]);
        fb.ins().store(MemFlags::trusted(), store_v, obj_v, off);
    }
    Ok(())
}
