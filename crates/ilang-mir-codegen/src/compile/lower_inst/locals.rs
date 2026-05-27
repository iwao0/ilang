//! Local-variable instruction lowering — `DefLocal`, `UseLocal`,
//! `AddrOfLocal`, `AddrOfField`. Each local either rides a
//! Cranelift `Variable` (SSA-style) or a `StackSlot` (when escape
//! analysis demanded an addressable slot).

#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::InstBuilder;
use cranelift_frontend::FunctionBuilder as ClifFnBuilder;
use cranelift_module::Module;

use ilang_mir::{Inst, ValueId};

use crate::ty::mir_to_clif;

use super::super::{CompileError, OBJECT_HEADER_BYTES};

pub(super) fn lower_local_inst<M: Module>(
    fb: &mut ClifFnBuilder,
    vmap: &mut HashMap<ValueId, Value>,
    _module: &mut M,
    prog_ctx: &super::super::ProgCtx,
    fn_ctx: &super::super::FnCtx,
    inst: &Inst,
) -> Result<(), CompileError> {
    let super::super::ProgCtx { prog, .. } = *prog_ctx;
    let super::super::FnCtx {
        func,
        locals,
        local_slots,
        ..
    } = *fn_ctx;
    match inst {
        Inst::DefLocal { local, value } => {
            let v = vmap[value];
            if std::env::var("ILANG_DEBUG_DEFLOCAL").is_ok() {
                let want = func.local_tys[local.0 as usize].clone();
                let got = fb.func.dfg.value_type(v);
                eprintln!(
                    "[deflocal] fn={} local#{} declared={want} clif_val_ty={got}",
                    func.name.as_str(),
                    local.0
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
        _ => unreachable!("lower_local_inst called with non-local inst"),
    }
    Ok(())
}
