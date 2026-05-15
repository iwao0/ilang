//! `Inst::LoadField` / `Inst::StoreField` lowering — the
//! second-biggest cluster after `Inst::Call`. Extracted from
//! `lower_inst/mod.rs`.

#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::{InstBuilder, Signature};
use cranelift_frontend::{FunctionBuilder as ClifFnBuilder, Variable};
use cranelift_module::{DataId, Module};

use ilang_ast::Symbol;
use ilang_mir::{
    FuncId, Function as MirFunction, MirTy, Program, StaticSlotId, ValueId,
};


use super::super::abi::{
    celem_clif_type_with_enum, elem_clif_type, extend_to_i64, ireduce_or_pass,
    reduce_from_i64,
};
use super::super::{
    CompileError, MapIds, PanicAux, PrintIds, PrintLits, PromiseIds, StrIds,
    OBJECT_HEADER_BYTES,
};

pub(super) fn lower_load_field<M: Module>(
    fb: &mut ClifFnBuilder,
    dst: &ValueId, obj: &ValueId, field: &ilang_mir::FieldId,
    vmap: &mut HashMap<ValueId, Value>,
    func: &MirFunction,
    _fn_ids: &HashMap<FuncId, cranelift_module::FuncId>,
    _builtin_ids: &HashMap<String, (cranelift_module::FuncId, Signature)>,
    _static_data: &HashMap<StaticSlotId, DataId>,
    _string_data: &HashMap<Symbol, DataId>,
    _alloc_id: cranelift_module::FuncId,
    _map_ids: MapIds,
    _promise_ids: PromiseIds,
    str_ids: StrIds,
    _print_ids: PrintIds,
    panic_aux: PanicAux,
    _print_lits: PrintLits,
    module: &mut M,
    _locals: &[Variable],
    prog: &Program,
    _env_value: Value,
    _class_global: &[u32],
    enum_global: &[u32],
    _class_struct_global: &[i64],
    _stack_local: &std::collections::HashSet<ValueId>,
) -> Result<(), CompileError> {
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
        let storage_ct = match elem_clif_type(&dst_ty_mir) {
            Some(t) if t.bits() <= 32 => t,
            _ => types::I32,
        };
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
        // discussed in the language design notes.
        if let MirTy::Enum(eid) = &dst_ty_mir {
            let layout = &prog.enums[eid.0 as usize];
            let unit_only = layout
                .variants
                .iter()
                .all(|v| matches!(v.payload, ilang_mir::VariantPayload::Unit));
            if unit_only {
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
                let f = module.declare_func_in_func(
                    panic_aux.enum_unit_get_checked,
                    fb.func,
                );
                let call = fb.ins().call(f, &[global_v, disc_i64]);
                let v = fb.inst_results(call)[0];
                vmap.insert(*dst, v);
                return Ok(());
            }
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
                    MirTy::Object(inner_cid) => matches!(
                        prog.classes[inner_cid.0 as usize].repr,
                        ilang_mir::ClassRepr::CRepr
                            | ilang_mir::ClassRepr::CPacked
                            | ilang_mir::ClassRepr::CUnion
                    ),
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
    obj: &ValueId, field: &ilang_mir::FieldId, value: &ValueId,
    vmap: &mut HashMap<ValueId, Value>,
    func: &MirFunction,
    _fn_ids: &HashMap<FuncId, cranelift_module::FuncId>,
    _builtin_ids: &HashMap<String, (cranelift_module::FuncId, Signature)>,
    _static_data: &HashMap<StaticSlotId, DataId>,
    _string_data: &HashMap<Symbol, DataId>,
    _alloc_id: cranelift_module::FuncId,
    _map_ids: MapIds,
    _promise_ids: PromiseIds,
    _str_ids: StrIds,
    _print_ids: PrintIds,
    _panic_aux: PanicAux,
    _print_lits: PrintLits,
    _module: &mut M,
    _locals: &[Variable],
    prog: &Program,
    _env_value: Value,
    _class_global: &[u32],
    _enum_global: &[u32],
    _class_struct_global: &[i64],
    _stack_local: &std::collections::HashSet<ValueId>,
) -> Result<(), CompileError> {
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
        let val_ty_mir = func.ty_of(*value).clone();
        let raw_val = vmap[value];
        let storage_ct = match elem_clif_type(&val_ty_mir) {
            Some(t) if t.bits() <= 32 => t,
            _ => types::I32,
        };
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
            if matches!(
                inner_layout.repr,
                ilang_mir::ClassRepr::CRepr
                    | ilang_mir::ClassRepr::CPacked
                    | ilang_mir::ClassRepr::CUnion
            ) {
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
        // Unit-only enum field: the SSA value is a heap-box
        // pointer; the C struct slot wants the underlying
        // discriminant. Load tag from the box (offset 0) and
        // narrow to the field's repr width before storing.
        let raw = if let MirTy::Enum(eid) = &val_ty_mir {
            let layout = &prog.enums[eid.0 as usize];
            let unit_only = layout
                .variants
                .iter()
                .all(|v| matches!(v.payload, ilang_mir::VariantPayload::Unit));
            if unit_only {
                fb.ins().load(types::I64, MemFlags::trusted(), raw, 0)
            } else {
                raw
            }
        } else {
            raw
        };
        match celem_clif_type_with_enum(prog, &val_ty_mir) {
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
