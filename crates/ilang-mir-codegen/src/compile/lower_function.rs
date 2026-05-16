//! Per-function lowering driver. Allocates the clif blocks /
//! variables that mirror the MIR function, then dispatches each
//! `Inst` through `lower_inst` and each `Terminator` through
//! `lower_term`.

use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::Signature;
use cranelift_frontend::{FunctionBuilder as ClifFnBuilder, Variable};
use cranelift_module::{DataId, Module};

use ilang_ast::Symbol;
use ilang_mir::{FuncId, Function as MirFunction, MirTy, Program, StaticSlotId, ValueId};

use crate::compile::abi::{struct_chunks, struct_hfa, struct_indirect};
use crate::ty::mir_to_clif;

use super::lower_inst::lower_inst;
use super::lower_term_const::{lower_term, ReturnAbi};
use super::{CompileError, MapIds, PanicAux, PrintIds, PrintLits, PromiseIds, StrIds};

pub(super) fn lower_function<M: Module>(
    fb: &mut ClifFnBuilder,
    func: &MirFunction,
    fn_ids: &HashMap<FuncId, cranelift_module::FuncId>,
    _fn_sigs: &HashMap<FuncId, Signature>,
    builtin_ids: &HashMap<String, (cranelift_module::FuncId, Signature)>,
    static_data: &HashMap<StaticSlotId, DataId>,
    string_data: &HashMap<Symbol, DataId>,
    alloc_id: cranelift_module::FuncId,
    map_ids: MapIds,
    promise_ids: PromiseIds,
    str_ids: StrIds,
    print_ids: PrintIds,
    panic_aux: PanicAux,
    print_lits: PrintLits,
    module: &mut M,
    prog: &Program,
    class_global: &[u32],
    enum_global: &[u32],
    class_struct_global: &[i64],
    stack_local: &std::collections::HashSet<ValueId>,
) -> Result<(), CompileError> {
    // Entry block has a special schema:
    //   [sret_ptr?] [param_0_chunks…] … [param_N_chunks…] [env_ptr (non-extern)]
    // where each `param_i_chunks…` is either:
    //   - HFA float regs (1–4 floats) for ≤4-field float-only CRepr
    //   - 1–2 i64 GPR chunks for ≤16 B CRepr
    //   - the sret pointer absorbs the return (no clif return)
    //   - or a single clif slot for non-CRepr (mir_to_clif).
    // Non-entry blocks keep the simple 1-slot-per-MIR-param shape:
    // SSA-style block args flowing between basic blocks are always
    // pointer-sized for Object types.
    let is_extern = matches!(func.kind, ilang_mir::FunctionKind::Extern { .. });
    let sret_ret_size = struct_indirect(&func.ret, prog);
    let mut blocks: Vec<cranelift::prelude::Block> = Vec::with_capacity(func.blocks.len());
    for (i, blk) in func.blocks.iter().enumerate() {
        let b = fb.create_block();
        if i == func.entry.0 as usize {
            // Sret hidden first param.
            if sret_ret_size.is_some() {
                fb.append_block_param(b, types::I64);
            }
            // Per-MIR-param: chunks / HFA / single slot.
            for &p in &blk.params {
                let pty = func.ty_of(p);
                if let Some((elem_ct, count)) = struct_hfa(pty, prog) {
                    for _ in 0..count {
                        fb.append_block_param(b, elem_ct);
                    }
                    continue;
                }
                if let Some(chunks) = struct_chunks(pty, prog) {
                    for _ in 0..chunks {
                        fb.append_block_param(b, types::I64);
                    }
                    continue;
                }
                if let Some(ct) = mir_to_clif(pty) {
                    fb.append_block_param(b, ct);
                }
            }
            // Trailing env-ptr for non-extern fns.
            if !is_extern {
                fb.append_block_param(b, types::I64);
            }
        } else {
            for &p in &blk.params {
                let pty = func.ty_of(p);
                if let Some(ct) = mir_to_clif(pty) {
                    fb.append_block_param(b, ct);
                }
            }
        }
        blocks.push(b);
    }

    let entry_clif = blocks[func.entry.0 as usize];
    fb.switch_to_block(entry_clif);
    fb.seal_block(entry_clif);

    // Map ValueId → cranelift Value. Non-entry blocks bind 1:1;
    // the entry block walks the schema to either bind a clif value
    // directly or reassemble chunks into a fresh stack buffer.
    let mut vmap: HashMap<ValueId, Value> = HashMap::new();
    let mut sret_ptr: Option<Value> = None;
    {
        let entry_bps: Vec<Value> = fb.block_params(entry_clif).to_vec();
        let mut clif_idx = 0usize;
        if sret_ret_size.is_some() {
            sret_ptr = Some(entry_bps[clif_idx]);
            clif_idx += 1;
        }
        for &p in &func.blocks[func.entry.0 as usize].params {
            let pty = func.ty_of(p);
            if let Some((elem_ct, count)) = struct_hfa(pty, prog) {
                // Reassemble HFA floats into a fresh stack buffer
                // and bind the param's ValueId to its pointer. The
                // buffer's lifetime is the callee's frame — exactly
                // what value semantics needs.
                let layout = match pty {
                    MirTy::Object(cid) => &prog.classes[cid.0 as usize],
                    _ => unreachable!(),
                };
                let slot = fb.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    layout.c_size.max(1) as u32,
                    3,
                ));
                let ptr = fb.ins().stack_addr(types::I64, slot, 0);
                let elem_size: i32 = if elem_ct == types::F32 { 4 } else { 8 };
                for c in 0..count {
                    fb.ins().store(
                        MemFlags::trusted(),
                        entry_bps[clif_idx + c],
                        ptr,
                        (c as i32) * elem_size,
                    );
                }
                clif_idx += count;
                vmap.insert(p, ptr);
                continue;
            }
            if let Some(chunks) = struct_chunks(pty, prog) {
                let layout = match pty {
                    MirTy::Object(cid) => &prog.classes[cid.0 as usize],
                    _ => unreachable!(),
                };
                let slot = fb.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    layout.c_size.max(1) as u32,
                    3,
                ));
                let ptr = fb.ins().stack_addr(types::I64, slot, 0);
                for c in 0..chunks {
                    fb.ins().store(
                        MemFlags::trusted(),
                        entry_bps[clif_idx + c],
                        ptr,
                        (c as i32) * 8,
                    );
                }
                clif_idx += chunks;
                vmap.insert(p, ptr);
                continue;
            }
            if mir_to_clif(pty).is_some() {
                vmap.insert(p, entry_bps[clif_idx]);
                clif_idx += 1;
            }
        }
        // Skip past the trailing env-ptr (consumed below).
        let _ = clif_idx;
    }
    // Non-entry blocks: 1-slot-per-MIR-param as before.
    for (i, blk) in func.blocks.iter().enumerate() {
        if i == func.entry.0 as usize {
            continue;
        }
        let cb = blocks[i];
        let mut clif_idx = 0;
        for &p in &blk.params {
            if mir_to_clif(func.ty_of(p)).is_some() {
                let cv = fb.block_params(cb)[clif_idx];
                vmap.insert(p, cv);
                clif_idx += 1;
            }
        }
    }

    // The hidden env-ptr is the entry block's last clif param (only
    // present for non-extern fns).
    let env_value: Value = if !is_extern {
        let bps = fb.block_params(entry_clif);
        bps[bps.len() - 1]
    } else {
        // Extern bodies don't have an env-ptr; use a constant 0 as a
        // placeholder. Extern bodies don't capture closures, so any
        // LoadCapture inside one would already be a MIR-level bug.
        fb.ins().iconst(types::I64, 0)
    };

    let ret_abi = ReturnAbi {
        sret_ptr,
        ret_ty: func.ret.clone(),
    };

    // Declare a Cranelift `Variable` for every MIR local. Cranelift
    // performs the on-demand SSA construction (block-arg insertion
    // for loop-carried values) once we use def_var / use_var.
    let mut locals: Vec<Variable> = Vec::with_capacity(func.local_tys.len());
    for lt in func.local_tys.iter() {
        let ct = mir_to_clif(lt).unwrap_or(types::I64);
        let var = fb.declare_var(ct);
        locals.push(var);
    }

    // Lower in MIR-block order. Any MIR block reachable from terminators
    // is sealed after we've emitted its branch sources — for the M1
    // subset (no irreducible CFG), it's safe to seal each block right
    // after emitting all its predecessors. We seal aggressively at the
    // end to cover the common case.
    for (i, blk) in func.blocks.iter().enumerate() {
        let cb = blocks[i];
        if i != func.entry.0 as usize {
            fb.switch_to_block(cb);
        }
        for inst in &blk.insts {
            lower_inst(
                fb,
                inst,
                &mut vmap,
                func,
                fn_ids,
                builtin_ids,
                static_data,
                string_data,
                alloc_id,
                map_ids,
                promise_ids,
                str_ids,
                print_ids,
                panic_aux,
                print_lits,
                module,
                &locals,
                prog,
                env_value,
                class_global,
                enum_global,
                class_struct_global,
                stack_local,
            )?;
        }
        lower_term(fb, &blk.term, &vmap, &blocks, &ret_abi, prog)?;
    }
    // Seal all blocks (M1 doesn't construct cycles via ssa add_predecessor;
    // every predecessor is already known by structure).
    for (i, _) in func.blocks.iter().enumerate() {
        if i != func.entry.0 as usize {
            fb.seal_block(blocks[i]);
        }
    }
    Ok(())
}
