//! MIR sanity checks. Run under debug builds to catch lowering bugs
//! early: SSA single-assignment, terminator presence, block-arg arity.

use std::collections::HashSet;

use crate::inst::{BlockId, Inst, Terminator, ValueId};
use crate::program::{Function, Program};

#[derive(Debug)]
pub struct ValidateError {
    pub function: String,
    pub message: String,
}

pub fn validate_program(p: &Program) -> Result<(), Vec<ValidateError>> {
    let mut errs = Vec::new();
    for f in &p.functions {
        if let Err(e) = validate_function(f) {
            errs.extend(e.into_iter().map(|m| ValidateError {
                function: f.display_name.to_string(),
                message: m,
            }));
        }
    }
    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}

pub fn validate_function(f: &Function) -> Result<(), Vec<String>> {
    let mut errs = Vec::new();
    let mut defined: HashSet<ValueId> = HashSet::new();

    // Function params alias the entry block's params (same SSA values).
    // Validate they actually appear as such.
    let entry = &f.blocks[f.entry.0 as usize];
    for p in f.params.iter() {
        if !entry.params.contains(&p.value) {
            errs.push(format!(
                "function param {:?} not in entry block params",
                p.value
            ));
        }
    }

    for (bi, blk) in f.blocks.iter().enumerate() {
        for &p in &blk.params {
            if !defined.insert(p) {
                errs.push(format!("block bb{bi} param {:?} double-defined", p));
            }
        }
        for inst in &blk.insts {
            for d in inst_defs(inst) {
                if !defined.insert(d) {
                    errs.push(format!("inst defines {:?} more than once", d));
                }
            }
        }
    }

    // All branch targets must point to existing blocks; arg counts
    // must match the destination block's params.
    for (bi, blk) in f.blocks.iter().enumerate() {
        match &blk.term {
            Terminator::Br { dst, args } => check_branch(f, &mut errs, bi, *dst, args),
            Terminator::CondBr {
                then_block, then_args, else_block, else_args, ..
            } => {
                check_branch(f, &mut errs, bi, *then_block, then_args);
                check_branch(f, &mut errs, bi, *else_block, else_args);
            }
            Terminator::Switch { cases, default, default_args, .. } => {
                check_branch(f, &mut errs, bi, *default, default_args);
                for c in cases.iter() {
                    check_branch(f, &mut errs, bi, c.dst, &c.args);
                }
            }
            Terminator::Return { .. } | Terminator::Unreachable => {}
        }
    }

    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}

fn check_branch(
    f: &Function,
    errs: &mut Vec<String>,
    src_block: usize,
    dst: BlockId,
    args: &[ValueId],
) {
    let Some(target) = f.blocks.get(dst.0 as usize) else {
        errs.push(format!(
            "bb{src_block} branches to missing block bb{}",
            dst.0
        ));
        return;
    };
    if target.params.len() != args.len() {
        errs.push(format!(
            "bb{src_block} -> bb{}: arg count {} != block params {}",
            dst.0,
            args.len(),
            target.params.len()
        ));
    }
}

fn inst_defs(inst: &Inst) -> Vec<ValueId> {
    match inst {
        Inst::Const { dst, .. }
        | Inst::ClosureSelf { dst }
        | Inst::BinOp { dst, .. }
        | Inst::UnOp { dst, .. }
        | Inst::Cast { dst, .. }
        | Inst::NewObject { dst, .. }
        | Inst::LoadField { dst, .. }
        | Inst::NewArray { dst, .. }
        | Inst::NewArrayEmpty { dst, .. }
        | Inst::NewSimd { dst, .. }
        | Inst::ArrayLen { dst, .. }
        | Inst::ArrayLoad { dst, .. }
        | Inst::NewMap { dst, .. }
        | Inst::MapGet { dst, .. }
        | Inst::NewTuple { dst, .. }
        | Inst::TupleExtract { dst, .. }
        | Inst::NewOptional { dst, .. }
        | Inst::OptionalIsSome { dst, .. }
        | Inst::OptionalUnwrap { dst, .. }
        | Inst::NewEnum { dst, .. }
        | Inst::EnumTag { dst, .. }
        | Inst::EnumDiscStr { dst, .. }
        | Inst::EnumPayload { dst, .. }
        | Inst::MakeClosure { dst, .. }
        | Inst::FuncAddr { dst, .. }
        | Inst::LoadCapture { dst, .. }
        | Inst::WeakUpgrade { dst, .. }
        | Inst::TypeOf { dst, .. }
        | Inst::IsInstance { dst, .. }
        | Inst::DowncastOrNone { dst, .. }
        | Inst::LoadStatic { dst, .. }
        | Inst::UseLocal { dst, .. }
        | Inst::AddrOfLocal { dst, .. }
        | Inst::AddrOfField { dst, .. } => vec![*dst],
        Inst::Call { dst, .. }
        | Inst::CallIndirect { dst, .. }
        | Inst::CallRawIndirect { dst, .. }
        | Inst::VirtCall { dst, .. }
        | Inst::ComCall { dst, .. } => {
            dst.iter().copied().collect()
        }
        Inst::StoreField { .. }
        | Inst::ArrayStore { .. }
        | Inst::MapSet { .. }
        | Inst::Retain { .. }
        | Inst::Release { .. }
        | Inst::WeakRetain { .. }
        | Inst::WeakRelease { .. }
        | Inst::StoreStatic { .. }
        | Inst::Panic { .. }
        | Inst::DefLocal { .. } => Vec::new(),
    }
}
