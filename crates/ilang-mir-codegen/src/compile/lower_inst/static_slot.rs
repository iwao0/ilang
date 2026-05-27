//! Static-slot instruction lowering — `LoadStatic`, `StoreStatic`.
//! Reads / writes a typed value from a process-wide data symbol; the
//! slot's `MirTy` decides whether the i64 cell needs a narrowing
//! `ireduce` (small ints / bool) or a `bitcast` (floats).

#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::InstBuilder;
use cranelift_frontend::FunctionBuilder as ClifFnBuilder;
use cranelift_module::Module;

use ilang_mir::{Inst, MirTy, ValueId};

use super::super::CompileError;

pub(super) fn lower_static_slot_inst<M: Module>(
    fb: &mut ClifFnBuilder,
    vmap: &mut HashMap<ValueId, Value>,
    module: &mut M,
    prog_ctx: &super::super::ProgCtx,
    _fn_ctx: &super::super::FnCtx,
    inst: &Inst,
) -> Result<(), CompileError> {
    let super::super::ProgCtx {
        static_data, prog, ..
    } = *prog_ctx;
    match inst {
        Inst::LoadStatic { dst, slot } => {
            let did = *static_data.get(slot).ok_or_else(|| {
                CompileError::Other(format!("missing static data slot #{}", slot.0))
            })?;
            let gv = module.declare_data_in_func(did, fb.func);
            let addr = fb.ins().symbol_value(types::I64, gv);
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
        _ => unreachable!("lower_static_slot_inst called with non-static inst"),
    }
    Ok(())
}
