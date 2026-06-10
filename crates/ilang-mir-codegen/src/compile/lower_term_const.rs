//! Cranelift lowering for MIR terminators and constants.

use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::InstBuilder;
use cranelift_frontend::FunctionBuilder as ClifFnBuilder;
use cranelift_module::Module;

use ilang_mir::{MirConst, MirTy, Program, Terminator, ValueId};

use crate::compile::abi::{
    struct_chunks_with_max, struct_hfa_ret, struct_indirect_with_max,
    struct_sret_for_internal,
};
use crate::ty::mir_to_clif;

use super::{CompileError, PanicAux};

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
    /// When `true`, every CRepr struct return uses sret regardless
    /// of size — mirrors the same flag in `clif_signature_for` /
    /// `lower_function`'s entry-block schema for internal fns.
    pub force_internal_sret: bool,
}

pub(super) fn lower_term<M: Module>(
    fb: &mut ClifFnBuilder,
    term: &Terminator,
    vmap: &HashMap<ValueId, Value>,
    blocks: &[cranelift::prelude::Block],
    ret_abi: &ReturnAbi,
    prog: &Program,
    module: &mut M,
    panic_aux: &PanicAux,
) -> Result<(), CompileError> {
    // Helper: keep only values that have a clif counterpart in vmap.
    let visible = |args: &[ValueId]| -> Vec<cranelift_codegen::ir::BlockArg> {
        args.iter()
            .filter_map(|a| vmap.get(a).copied().map(|v| v.into()))
            .collect()
    };
    match term {
        Terminator::Return { value, release_value } => {
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
            // `release_value=true` means the buffer `value` points
            // at was callee-allocated for the return — after the
            // ABI consumes its bytes, free it via `__mir_free` to
            // close the alloc/free pair. Borrowed return values
            // (`release_value=false`) leave the buffer alone; its
            // lifetime belongs upstream.
            //
            // Non-CRepr returns fall through to the original
            // single-value or void return path.
            if let Some(v) = value {
                let cv_opt = vmap.get(v).copied();
                // Return-shape decision MUST mirror
                // `clif_signature_for` / `lower_function`'s entry
                // schema: HFA float regs first, then GPR chunks,
                // then indirect sret. Picking sret here for a
                // function whose signature actually returns
                // floats directly would memcpy into a non-existent
                // hidden pointer and the caller would see zeros.
                let call_conv = fb.func.signature.call_conv;
                let ret_hfa = if ret_abi.force_internal_sret {
                    None
                } else {
                    struct_hfa_ret(&ret_abi.ret_ty, prog, call_conv)
                };
                let sret_size = if ret_hfa.is_some() {
                    None
                } else if ret_abi.force_internal_sret {
                    struct_sret_for_internal(&ret_abi.ret_ty, prog)
                } else {
                    struct_indirect_with_max(&ret_abi.ret_ty, prog, ret_abi.chunk_max)
                };
                if let Some((elem_ct, count)) = ret_hfa {
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
                        if *release_value {
                            emit_free_crepr_buffer(fb, module, panic_aux, cv, &ret_abi.ret_ty, prog);
                        }
                        fb.ins().return_(&vs);
                        return Ok(());
                    }
                }
                if let Some(c_size) = sret_size {
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
                        if *release_value {
                            emit_free_crepr_buffer(fb, module, panic_aux, cv, &ret_abi.ret_ty, prog);
                        }
                        fb.ins().return_(&[]);
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
                        if *release_value {
                            emit_free_crepr_buffer(fb, module, panic_aux, cv, &ret_abi.ret_ty, prog);
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

/// Free the CRepr struct buffer that a `Return [release]` is handing
/// off to the caller via sret memcpy / chunk load / HFA spread.
/// Mirrors the CRepr branch of `lower_arc_inst::Release` — the
/// return-side already consumed the bytes by this point, so the
/// callee-side `__mir_alloc`'d buffer is free to release.
fn emit_free_crepr_buffer<M: Module>(
    fb: &mut ClifFnBuilder,
    module: &mut M,
    panic_aux: &PanicAux,
    cv: Value,
    ret_ty: &MirTy,
    prog: &Program,
) {
    if let MirTy::Object(cid) = ret_ty {
        let layout = &prog.classes[cid.0 as usize];
        let sz = layout.c_size.max(1);
        let sz_v = fb.ins().iconst(types::I64, sz);
        let r = module.declare_func_in_func(panic_aux.mir_free, fb.func);
        fb.ins().call(r, &[cv, sz_v]);
    }
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
