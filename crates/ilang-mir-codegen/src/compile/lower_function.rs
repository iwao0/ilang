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
use ilang_mir::{FuncId, Function as MirFunction, Program, StaticSlotId, ValueId};

use crate::ty::mir_to_clif;

use super::lower_inst::lower_inst;
use super::lower_term_const::lower_term;
use super::{CompileError, MapIds, PanicAux, PrintIds, PrintLits, StrIds};

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
    // Allocate clif blocks 1:1 with MIR blocks. Skip Unit-typed
    // block params at the clif level since clif has no unit type;
    // the matching ValueIds get a sentinel (i8 0) at use-sites.
    let mut blocks: Vec<cranelift::prelude::Block> = Vec::with_capacity(func.blocks.len());
    for (i, blk) in func.blocks.iter().enumerate() {
        let b = fb.create_block();
        for &p in &blk.params {
            let pty = func.ty_of(p);
            if let Some(ct) = mir_to_clif(pty) {
                fb.append_block_param(b, ct);
            }
        }
        // The entry block carries the hidden env-pointer param last
        // (matching the unified clif signature in `clif_signature_for`).
        if i == func.entry.0 as usize {
            fb.append_block_param(b, types::I64);
        }
        blocks.push(b);
    }

    // Map ValueId → cranelift Value. Unit-typed values aren't bound
    // (use-sites filter them out).
    let mut vmap: HashMap<ValueId, Value> = HashMap::new();
    for (i, blk) in func.blocks.iter().enumerate() {
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

    let entry_clif = blocks[func.entry.0 as usize];
    fb.switch_to_block(entry_clif);
    fb.seal_block(entry_clif);

    // The hidden env-ptr is the entry block's last clif param.
    let env_value: Value = {
        let bps = fb.block_params(entry_clif);
        bps[bps.len() - 1]
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
        lower_term(fb, &blk.term, &vmap, &blocks)?;
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
