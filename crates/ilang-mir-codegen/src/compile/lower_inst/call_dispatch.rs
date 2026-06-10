//! Indirect-call instruction lowering — `VirtCall`, `CallIndirect`,
//! `CallRawIndirect`, `ComCall`. Sibling to `calls::lower_call`
//! (which handles direct `Inst::Call`).

#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::{AbiParam, InstBuilder};
use cranelift_frontend::FunctionBuilder as ClifFnBuilder;
use cranelift_module::Module;

use ilang_mir::{Inst, MirTy, ValueId};

use crate::ty::mir_to_clif;

use super::super::abi::{
    struct_chunks_with_max, struct_hfa, struct_sret_for_internal, IL_BYVAL_CHUNK_MAX,
};
use super::super::CompileError;

pub(super) fn lower_call_dispatch_inst<M: Module>(
    fb: &mut ClifFnBuilder,
    vmap: &mut HashMap<ValueId, Value>,
    module: &mut M,
    prog_ctx: &super::super::ProgCtx,
    fn_ctx: &super::super::FnCtx,
    inst: &Inst,
) -> Result<(), CompileError> {
    let super::super::ProgCtx {
        alloc_id,
        str_ids,
        prog,
        ..
    } = *prog_ctx;
    let super::super::FnCtx { func, .. } = *fn_ctx;
    match inst {
        Inst::VirtCall { dst, recv, slot, args } => {
            // Load class_id from object header, dispatch via the
            // host runtime helper, then call_indirect. See the
            // comment in the original arm for the chunked-CRepr
            // gotcha.
            let recv_v = vmap[recv];
            let cid_v = fb.ins().load(types::I64, MemFlags::trusted(), recv_v, 0);
            let slot_v = fb.ins().iconst(types::I64, slot.0 as i64);
            let dispatch_ref = module.declare_func_in_func(str_ids.virt_dispatch, fb.func);
            let lookup = fb.ins().call(dispatch_ref, &[cid_v, slot_v]);
            let fn_ptr = fb.inst_results(lookup)[0];

            let chunk_max = IL_BYVAL_CHUNK_MAX;
            let mut clif_sig = module.make_signature();
            let mut arg_vs: Vec<Value> = Vec::with_capacity(args.len() + 2);

            let dst_ty_mir = dst.map(|d| func.ty_of(d).clone());
            // vtable に乗る method は構造的に全て `FunctionKind::Local`
            // (継承で override される method も Local の派生)。 直接
            // call と同じ「内部 fn は CRepr struct return を全サイズ
            // sret 強制」を VirtCall でも揃える。 揃えないと caller
            // signature (chunks return) と callee signature (sret) が
            // ミスマッチして Cranelift が garbage 値を返し、 debug
            // ビルドで SIGSEGV する (NSRange 16 byte 戻り値の
            // `cal.rangeOfUnit(...)` で再現)。 サイズしきい値ベースの
            // `struct_indirect_with_max` ではなく
            // `struct_sret_for_internal` を使う。
            let sret_dst = if let Some(t) = &dst_ty_mir {
                if let Some(c_size) = struct_sret_for_internal(t, prog) {
                    clif_sig.params.push(AbiParam::special(
                        types::I64,
                        cranelift_codegen::ir::ArgumentPurpose::StructReturn,
                    ));
                    let size_v = fb.ins().iconst(types::I64, c_size);
                    let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
                    let alloc_call = fb.ins().call(alloc_ref, &[size_v]);
                    let ptr = fb.inst_results(alloc_call)[0];
                    arg_vs.push(ptr);
                    Some((dst.unwrap(), ptr))
                } else {
                    None
                }
            } else {
                None
            };
            let _ = chunk_max;

            // Receiver (this): always a pointer.
            clif_sig.params.push(AbiParam::new(types::I64));
            arg_vs.push(recv_v);

            // Per-arg by-value expansion.
            for a in args.iter() {
                let aty = func.ty_of(*a);
                let av = vmap[a];
                if let Some((elem_ct, count)) = struct_hfa(aty, prog) {
                    let elem_size: i32 = if elem_ct == types::F32 { 4 } else { 8 };
                    for c in 0..count {
                        clif_sig.params.push(AbiParam::new(elem_ct));
                        let v = fb.ins().load(
                            elem_ct,
                            MemFlags::trusted(),
                            av,
                            (c as i32) * elem_size,
                        );
                        arg_vs.push(v);
                    }
                    continue;
                }
                if let Some(chunks) = struct_chunks_with_max(aty, prog, chunk_max) {
                    for c in 0..chunks {
                        clif_sig.params.push(AbiParam::new(types::I64));
                        let cell = fb.ins().load(
                            types::I64,
                            MemFlags::trusted(),
                            av,
                            (c as i32) * 8,
                        );
                        arg_vs.push(cell);
                    }
                    continue;
                }
                // Plain arg: 1 clif slot, value as-is.
                let ct = fb.func.dfg.value_type(av);
                clif_sig.params.push(AbiParam::new(ct));
                arg_vs.push(av);
            }

            // Trailing env-ptr (Local fns always carry one).
            clif_sig.params.push(AbiParam::new(types::I64));
            let zero = fb.ins().iconst(types::I64, 0);
            arg_vs.push(zero);

            // Return shape.
            if sret_dst.is_none() {
                if let Some(t) = &dst_ty_mir {
                    if !matches!(t, MirTy::Unit) {
                        if let Some((elem_ct, count)) = struct_hfa(t, prog) {
                            for _ in 0..count {
                                clif_sig.returns.push(AbiParam::new(elem_ct));
                            }
                        } else if let Some(chunks) =
                            struct_chunks_with_max(t, prog, chunk_max)
                        {
                            for _ in 0..chunks {
                                clif_sig.returns.push(AbiParam::new(types::I64));
                            }
                        } else if let Some(ct) = mir_to_clif(t) {
                            clif_sig.returns.push(AbiParam::new(ct));
                        }
                    }
                }
            }

            let sig_ref = fb.import_signature(clif_sig);
            let inst_ref = fb.ins().call_indirect(sig_ref, fn_ptr, &arg_vs);

            // Reassemble the return into the dst value.
            if let Some((d, ptr)) = sret_dst {
                vmap.insert(d, ptr);
            } else if let Some(d) = dst {
                let dst_ty = func.ty_of(*d).clone();
                if let Some((elem_ct, count)) = struct_hfa(&dst_ty, prog) {
                    // HFA return → reassemble floats into a heap buffer.
                    let layout = if let MirTy::Object(cid) = &dst_ty {
                        &prog.classes[cid.0 as usize]
                    } else { unreachable!() };
                    let size_v = fb.ins().iconst(types::I64, layout.c_size.max(1));
                    let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
                    let alloc_call = fb.ins().call(alloc_ref, &[size_v]);
                    let ptr = fb.inst_results(alloc_call)[0];
                    let results: Vec<Value> = fb.inst_results(inst_ref).to_vec();
                    let elem_size: i32 = if elem_ct == types::F32 { 4 } else { 8 };
                    for (i, &v) in results.iter().take(count).enumerate() {
                        fb.ins().store(
                            MemFlags::trusted(),
                            v,
                            ptr,
                            (i as i32) * elem_size,
                        );
                    }
                    vmap.insert(*d, ptr);
                } else if let Some(chunks) = struct_chunks_with_max(&dst_ty, prog, chunk_max) {
                    let layout = if let MirTy::Object(cid) = &dst_ty {
                        &prog.classes[cid.0 as usize]
                    } else { unreachable!() };
                    let size_v = fb.ins().iconst(types::I64, layout.c_size.max(1));
                    let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
                    let alloc_call = fb.ins().call(alloc_ref, &[size_v]);
                    let ptr = fb.inst_results(alloc_call)[0];
                    let results: Vec<Value> = fb.inst_results(inst_ref).to_vec();
                    for (i, &cell) in results.iter().take(chunks).enumerate() {
                        fb.ins().store(
                            MemFlags::trusted(),
                            cell,
                            ptr,
                            (i as i32) * 8,
                        );
                    }
                    vmap.insert(*d, ptr);
                } else {
                    let results = fb.inst_results(inst_ref);
                    if let Some(&v) = results.first() {
                        vmap.insert(*d, v);
                    }
                }
            }
        }
        Inst::CallIndirect { dst, callee, sig, args } => {
            // Closure value: pointer to `[fn_ptr | captures...]`.
            // The wrapped fn is always internal (Trampoline /
            // Local-kind FnExpr lowering) — neither Extern nor
            // ExternBody is closurified — so the call site uses the
            // same "internal-fn CRepr struct return → sret" rule as
            // `Inst::Call` (calls.rs) and `Inst::VirtCall` above.
            // Without this, a `let f = make_box; f(...)` where
            // `make_box` returns a CRepr struct mismatches the
            // callee's sret signature and SIGSEGVs.
            let closure = vmap[callee];
            let fn_ptr = fb.ins().load(types::I64, MemFlags::trusted(), closure, 0);
            let chunk_max = IL_BYVAL_CHUNK_MAX;
            let mut clif_sig = module.make_signature();
            let mut arg_vs: Vec<Value> = Vec::with_capacity(args.len() + 2);
            // sret-first: alloc the destination buffer and prepend the
            // hidden pointer as the first clif arg.
            let sret_dst = if let Some(d) = dst {
                let dst_ty = func.ty_of(*d).clone();
                if let Some(c_size) = struct_sret_for_internal(&dst_ty, prog) {
                    clif_sig.params.push(AbiParam::special(
                        types::I64,
                        cranelift_codegen::ir::ArgumentPurpose::StructReturn,
                    ));
                    let size_v = fb.ins().iconst(types::I64, c_size);
                    let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
                    let alloc_call = fb.ins().call(alloc_ref, &[size_v]);
                    let ptr = fb.inst_results(alloc_call)[0];
                    arg_vs.push(ptr);
                    Some((*d, ptr))
                } else {
                    None
                }
            } else {
                None
            };
            // Per-param by-value expansion (mirrors VirtCall).
            for (p, a) in sig.params.iter().zip(args.iter()) {
                let av = vmap[a];
                if let Some((elem_ct, count)) = struct_hfa(p, prog) {
                    let elem_size: i32 = if elem_ct == types::F32 { 4 } else { 8 };
                    for c in 0..count {
                        clif_sig.params.push(AbiParam::new(elem_ct));
                        let v = fb.ins().load(
                            elem_ct,
                            MemFlags::trusted(),
                            av,
                            (c as i32) * elem_size,
                        );
                        arg_vs.push(v);
                    }
                    continue;
                }
                if let Some(chunks) = struct_chunks_with_max(p, prog, chunk_max) {
                    for c in 0..chunks {
                        clif_sig.params.push(AbiParam::new(types::I64));
                        let cell = fb.ins().load(
                            types::I64,
                            MemFlags::trusted(),
                            av,
                            (c as i32) * 8,
                        );
                        arg_vs.push(cell);
                    }
                    continue;
                }
                if let Some(ct) = mir_to_clif(p) {
                    clif_sig.params.push(AbiParam::new(ct));
                    arg_vs.push(av);
                }
            }
            // Trailing env-ptr (closure block pointer).
            clif_sig.params.push(AbiParam::new(types::I64));
            arg_vs.push(closure);
            // Return shape: empty if sret took the value; otherwise
            // a single clif slot. CRepr struct returns are always
            // sret-routed above, so the chunk / HFA fallback paths
            // never trigger here.
            if sret_dst.is_none() && !matches!(sig.ret, MirTy::Unit) {
                if let Some(ct) = mir_to_clif(&sig.ret) {
                    clif_sig.returns.push(AbiParam::new(ct));
                }
            }
            let sig_ref = fb.import_signature(clif_sig);
            let inst_ref = fb.ins().call_indirect(sig_ref, fn_ptr, &arg_vs);
            if let Some((d, ptr)) = sret_dst {
                vmap.insert(d, ptr);
            } else if let Some(d) = dst {
                let results = fb.inst_results(inst_ref);
                if let Some(&v) = results.first() {
                    vmap.insert(*d, v);
                }
            }
        }
        Inst::CallRawIndirect { dst, callee, sig, args } => {
            // Raw C function pointer: the value itself IS the fn pointer.
            // No fn_ptr load, no trailing env arg.
            let fn_ptr = vmap[callee];
            let mut clif_sig = module.make_signature();
            for p in sig.params.iter() {
                if let Some(ct) = mir_to_clif(p) {
                    clif_sig.params.push(AbiParam::new(ct));
                }
            }
            if !matches!(sig.ret, MirTy::Unit) {
                if let Some(ct) = mir_to_clif(&sig.ret) {
                    clif_sig.returns.push(AbiParam::new(ct));
                }
            }
            let sig_ref = fb.import_signature(clif_sig);
            let arg_vs: Vec<Value> = args.iter().map(|a| vmap[a]).collect();
            let inst_ref = fb.ins().call_indirect(sig_ref, fn_ptr, &arg_vs);
            if let Some(d) = dst {
                let results = fb.inst_results(inst_ref);
                if let Some(&v) = results.first() {
                    vmap.insert(*d, v);
                }
            }
        }
        Inst::ComCall { dst, recv, slot, sig, args } => {
            // COM vtable dispatch:
            //   vt = *(i64*)recv
            //   fp = *(i64*)(vt + slot * 8)
            //   fp(recv, args...)
            //
            // `recv` is passed as the first argument (the C `this`
            // pointer). The MIR-side `args` list does NOT include
            // it — we prepend here.
            let recv_v = vmap[recv];
            let vt = fb.ins().load(types::I64, MemFlags::trusted(), recv_v, 0);
            let slot_off = (*slot as i32) * 8;
            let fp = fb.ins().load(types::I64, MemFlags::trusted(), vt, slot_off);
            let mut clif_sig = module.make_signature();
            // Inherit the enclosing fn's call convention so stack-arg
            // / shadow-store placement matches what the COM method
            // expects (WindowsFastcall on x64 MSVC, SystemV on Linux).
            clif_sig.call_conv = fb.func.signature.call_conv;
            for p in sig.params.iter() {
                if let Some(ct) = mir_to_clif(p) {
                    clif_sig.params.push(AbiParam::new(ct));
                }
            }
            if !matches!(sig.ret, MirTy::Unit) {
                if let Some(ct) = mir_to_clif(&sig.ret) {
                    clif_sig.returns.push(AbiParam::new(ct));
                }
            }
            let sig_ref = fb.import_signature(clif_sig);
            let mut arg_vs: Vec<Value> = Vec::with_capacity(args.len() + 1);
            arg_vs.push(recv_v);
            for a in args.iter() {
                arg_vs.push(vmap[a]);
            }
            let inst_ref = fb.ins().call_indirect(sig_ref, fp, &arg_vs);
            if let Some(d) = dst {
                let results = fb.inst_results(inst_ref);
                if let Some(&v) = results.first() {
                    vmap.insert(*d, v);
                }
            }
        }
        _ => unreachable!("lower_call_dispatch_inst called with non-dispatch inst"),
    }
    Ok(())
}
