//! Cranelift lowering for MIR terminators and constants.

use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::InstBuilder;
use cranelift_frontend::FunctionBuilder as ClifFnBuilder;

use ilang_mir::{MirConst, MirTy, Terminator, ValueId};

use crate::ty::mir_to_clif;

use super::CompileError;

pub(super) fn lower_term(
    fb: &mut ClifFnBuilder,
    term: &Terminator,
    vmap: &HashMap<ValueId, Value>,
    blocks: &[cranelift::prelude::Block],
) -> Result<(), CompileError> {
    // Helper: keep only values that have a clif counterpart in vmap.
    let visible = |args: &[ValueId]| -> Vec<cranelift_codegen::ir::BlockArg> {
        args.iter()
            .filter_map(|a| vmap.get(a).copied().map(|v| v.into()))
            .collect()
    };
    match term {
        Terminator::Return { value } => {
            match value.and_then(|v| vmap.get(&v).copied()) {
                Some(cv) => {
                    fb.ins().return_(&[cv]);
                }
                None => {
                    fb.ins().return_(&[]);
                }
            }
        }
        Terminator::Br { dst, args } => {
            let cb = blocks[dst.0 as usize];
            let avs = visible(args);
            fb.ins().jump(cb, avs.iter());
        }
        Terminator::CondBr {
            cond, then_block, then_args, else_block, else_args,
        } => {
            let c = vmap[cond];
            let tb = blocks[then_block.0 as usize];
            let eb = blocks[else_block.0 as usize];
            let ta = visible(then_args);
            let ea = visible(else_args);
            fb.ins().brif(c, tb, ta.iter(), eb, ea.iter());
        }
        Terminator::Switch { scrutinee, cases, default, default_args } => {
            let s = vmap[scrutinee];
            let stype = fb.func.dfg.value_type(s);
            for c in cases.iter() {
                let lit = fb.ins().iconst(stype, c.value);
                let cmp = fb.ins().icmp(IntCC::Equal, s, lit);
                let target = blocks[c.dst.0 as usize];
                let next = fb.create_block();
                let target_args = visible(&c.args);
                fb.ins().brif(cmp, target, target_args.iter(), next, &[]);
                fb.switch_to_block(next);
                fb.seal_block(next);
            }
            let dst_blk = blocks[default.0 as usize];
            let dargs = visible(default_args);
            fb.ins().jump(dst_blk, dargs.iter());
        }
        Terminator::Unreachable => {
            fb.ins().trap(TrapCode::user(1).unwrap());
        }
    }
    Ok(())
}

pub(super) fn lower_const(
    fb: &mut ClifFnBuilder,
    c: &MirConst,
    ty: &MirTy,
) -> Result<Value, CompileError> {
    let ct = mir_to_clif(ty).ok_or(CompileError::Unsupported("unit const"))?;
    Ok(match c {
        MirConst::Bool(b) => fb.ins().iconst(ct, if *b { 1 } else { 0 }),
        MirConst::Int(n) => fb.ins().iconst(ct, *n),
        MirConst::F32(bits) => fb.ins().f32const(f32::from_bits(*bits)),
        MirConst::F64(bits) => fb.ins().f64const(f64::from_bits(*bits)),
        MirConst::Unit => return Err(CompileError::Unsupported("unit const")),
        MirConst::None => fb.ins().iconst(types::I64, 0),
        MirConst::Str(_) => return Err(CompileError::Unsupported("string const")),
    })
}
