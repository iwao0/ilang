//! Cranelift lowering for MIR terminators and constants.

use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::InstBuilder;
use cranelift_frontend::FunctionBuilder as ClifFnBuilder;

use ilang_mir::{MirConst, MirTy, Program, Terminator, ValueId};

use crate::compile::abi::{struct_chunks_with_max, struct_hfa, struct_indirect_with_max};
use crate::ty::mir_to_clif;

use super::CompileError;

/// Side-channel that `lower_term` consults when lowering a
/// `Terminator::Return`. The CRepr by-value ABI splits the return
/// value into HFA float regs / i64 chunks / sret hidden pointer
/// depending on the type's layout, so the bare `return cv` shape
/// isn't enough — the terminator needs to know the function's
/// return type and (for sret) the buffer the caller pre-allocated.
pub(super) struct ReturnAbi {
    pub sret_ptr: Option<Value>,
    pub ret_ty: MirTy,
    /// By-value chunk byte cap for this function's ABI (C vs ilang).
    /// Determines whether the return's bytes get split into i64
    /// chunks here or were absorbed by the sret hidden pointer
    /// at the entry block.
    pub chunk_max: i64,
}

pub(super) fn lower_term(
    fb: &mut ClifFnBuilder,
    term: &Terminator,
    vmap: &HashMap<ValueId, Value>,
    blocks: &[cranelift::prelude::Block],
    ret_abi: &ReturnAbi,
    prog: &Program,
) -> Result<(), CompileError> {
    // Helper: keep only values that have a clif counterpart in vmap.
    let visible = |args: &[ValueId]| -> Vec<cranelift_codegen::ir::BlockArg> {
        args.iter()
            .filter_map(|a| vmap.get(a).copied().map(|v| v.into()))
            .collect()
    };
    match term {
        Terminator::Return { value } => {
            // CRepr return paths: split bytes into the matching
            // clif-level return shape.
            //
            // - sret: copy struct bytes into the caller's pre-
            //   allocated buffer (`ret_abi.sret_ptr`); the clif
            //   function returns nothing.
            // - HFA: load each float field from the struct body
            //   and return them as multiple clif results.
            // - chunks: load 1 or 2 i64 cells from the struct body
            //   and return them.
            //
            // Non-CRepr returns fall through to the original
            // single-value or void return path.
            if let Some(v) = value {
                let cv_opt = vmap.get(v).copied();
                if let Some(c_size) =
                    struct_indirect_with_max(&ret_abi.ret_ty, prog, ret_abi.chunk_max)
                {
                    if let (Some(sret), Some(cv)) = (ret_abi.sret_ptr, cv_opt) {
                        // memcpy struct bytes (cv → sret) one i64 cell
                        // at a time. `c_size` already includes any
                        // tail padding (it rounds up to the largest
                        // field's alignment), so 8-byte stores are
                        // safe.
                        let mut off: i32 = 0;
                        while (off as i64) < c_size {
                            let cell = fb.ins().load(types::I64, MemFlags::trusted(), cv, off);
                            fb.ins().store(MemFlags::trusted(), cell, sret, off);
                            off += 8;
                        }
                        fb.ins().return_(&[]);
                        return Ok(());
                    }
                }
                if let Some((elem_ct, count)) = struct_hfa(&ret_abi.ret_ty, prog) {
                    if let Some(cv) = cv_opt {
                        let elem_size: i32 = if elem_ct == types::F32 { 4 } else { 8 };
                        let mut vs: Vec<Value> = Vec::with_capacity(count);
                        for c in 0..count {
                            vs.push(fb.ins().load(
                                elem_ct,
                                MemFlags::trusted(),
                                cv,
                                (c as i32) * elem_size,
                            ));
                        }
                        fb.ins().return_(&vs);
                        return Ok(());
                    }
                }
                if let Some(chunks) =
                    struct_chunks_with_max(&ret_abi.ret_ty, prog, ret_abi.chunk_max)
                {
                    if let Some(cv) = cv_opt {
                        let mut vs: Vec<Value> = Vec::with_capacity(chunks);
                        for c in 0..chunks {
                            vs.push(fb.ins().load(
                                types::I64,
                                MemFlags::trusted(),
                                cv,
                                (c as i32) * 8,
                            ));
                        }
                        fb.ins().return_(&vs);
                        return Ok(());
                    }
                }
            }
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
