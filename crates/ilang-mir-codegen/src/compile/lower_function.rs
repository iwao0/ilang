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

use crate::compile::abi::{
    chunk_max_for, ret_chunk_max, struct_chunks_with_max, struct_hfa, struct_hfa_ret,
    struct_indirect_with_max,
};
use crate::ty::mir_to_clif;

use super::lower_inst::lower_inst;
use super::lower_term_const::{lower_term, ReturnAbi};
use super::{CompileError, FmtIds, MapIds, PanicAux, PrintIds, PrintLits, PromiseIds, SetIds, StrIds};

pub(super) fn lower_function<M: Module>(
    fb: &mut ClifFnBuilder,
    func: &MirFunction,
    fn_ids: &HashMap<FuncId, cranelift_module::FuncId>,
    _fn_sigs: &HashMap<FuncId, Signature>,
    extern_alias_fn_ids: &std::collections::HashSet<FuncId>,
    builtin_ids: &HashMap<String, (cranelift_module::FuncId, Signature)>,
    static_data: &HashMap<StaticSlotId, DataId>,
    string_data: &HashMap<Symbol, DataId>,
    alloc_id: cranelift_module::FuncId,
    map_ids: MapIds,
    set_ids: SetIds,
    promise_ids: PromiseIds,
    str_ids: StrIds,
    print_ids: PrintIds,
    fmt_ids: FmtIds,
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
    //   - 1..N i64 GPR chunks for CRepr ≤ `chunk_max_for(func)` bytes
    //   - the sret pointer absorbs the return (no clif return)
    //   - or a single clif slot for non-CRepr (mir_to_clif).
    // The `chunk_max` differs per ABI: C ABI is fixed at 16 B by
    // the platform spec; ilang ABI uses `IL_BYVAL_CHUNK_MAX` so
    // moderate-sized structs ride in registers across pure-ilang
    // call boundaries.
    //
    // Non-entry blocks keep the simple 1-slot-per-MIR-param shape:
    // SSA-style block args flowing between basic blocks are always
    // pointer-sized for Object types.
    let is_extern = matches!(func.kind, ilang_mir::FunctionKind::Extern { .. });
    let chunk_max = chunk_max_for(func);
    // HFA param/return spreading is only valid on System V / AArch64.
    // Windows fastcall allows one register per arg/return, so HFA
    // is skipped there and float structs fall through to i64 chunks.
    let hfa_ok = fb.func.signature.call_conv
        != cranelift_codegen::isa::CallConv::WindowsFastcall;
    // Return-shape decision MUST mirror the one in
    // `clif_signature_for`: HFA float regs first (4 floats fit
    // even when c_size > chunk_max — e.g. NSRect's 4 doubles),
    // then GPR chunks, then indirect sret. Skipping the HFA
    // pre-check would synthesise a hidden sret pointer for a
    // function whose signature actually returns the floats
    // directly, mis-aligning the entry-block param schema.
    // Returns are register-bound; use the (possibly tighter) return
    // caps so this matches `clif_signature_for` on every ABI.
    let ret_max = ret_chunk_max(fb.func.signature.call_conv, chunk_max);
    let ret_hfa = struct_hfa_ret(&func.ret, prog, fb.func.signature.call_conv);
    let sret_ret_size = if ret_hfa.is_some() {
        None
    } else {
        struct_indirect_with_max(&func.ret, prog, ret_max)
    };
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
                if hfa_ok {
                    if let Some((elem_ct, count)) = struct_hfa(pty, prog) {
                        for _ in 0..count {
                            fb.append_block_param(b, elem_ct);
                        }
                        continue;
                    }
                }
                if let Some(chunks) = struct_chunks_with_max(pty, prog, chunk_max) {
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
            if hfa_ok {
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
            }
            if let Some(chunks) = struct_chunks_with_max(pty, prog, chunk_max) {
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
        // Return-side cap (SysV-tightened) so the Ret lowering picks
        // the same sret-vs-chunks shape as the signature / entry block.
        chunk_max: ret_max,
    };

    // Address-taken locals (those that show up in an
    // `Inst::AddrOfLocal`) need a stable memory address, so we back
    // them with a Cranelift `StackSlot` instead of an SSA Variable.
    // `DefLocal`/`UseLocal`/`AddrOfLocal` route through the slot via
    // `stack_store` / `stack_load` / `stack_addr`.
    let mut addr_taken_locals: std::collections::HashSet<ilang_mir::LocalId> =
        std::collections::HashSet::new();
    for blk in &func.blocks {
        for inst in &blk.insts {
            if let ilang_mir::Inst::AddrOfLocal { local, .. } = inst {
                addr_taken_locals.insert(*local);
            }
        }
    }

    // Declare a Cranelift `Variable` for every MIR local. Cranelift
    // performs the on-demand SSA construction (block-arg insertion
    // for loop-carried values) once we use def_var / use_var.
    // For address-taken locals we also allocate a StackSlot; the
    // Variable still exists as a placeholder but isn't read.
    let mut locals: Vec<Variable> = Vec::with_capacity(func.local_tys.len());
    let mut local_slots: Vec<Option<cranelift_codegen::ir::StackSlot>> =
        Vec::with_capacity(func.local_tys.len());
    for (idx, lt) in func.local_tys.iter().enumerate() {
        let ct = mir_to_clif(lt).unwrap_or(types::I64);
        let var = fb.declare_var(ct);
        locals.push(var);
        let lid = ilang_mir::LocalId(idx as u32);
        let slot = if addr_taken_locals.contains(&lid) {
            let size = ct.bytes().max(1);
            let align = match size {
                1 => 0,
                2 => 1,
                4 => 2,
                _ => 3,
            };
            Some(fb.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                size,
                align,
            )))
        } else {
            None
        };
        local_slots.push(slot);
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
        let prog_ctx = super::ProgCtx {
            fn_ids,
            extern_alias_fn_ids,
            builtin_ids,
            static_data,
            string_data,
            alloc_id,
            map_ids,
            set_ids,
            promise_ids,
            str_ids,
            print_ids,
            fmt_ids,
            panic_aux,
            print_lits,
            prog,
            class_global,
            enum_global,
            class_struct_global,
        };
        let fn_ctx = super::FnCtx {
            func,
            locals: &locals,
            local_slots: &local_slots,
            env_value,
            stack_local,
        };
        for inst in &blk.insts {
            lower_inst(fb, &mut vmap, module, &prog_ctx, &fn_ctx, inst)?;
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
