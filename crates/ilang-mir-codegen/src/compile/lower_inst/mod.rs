//! Per-instruction MIR → cranelift lowering. The bulk of `compile/`
//! lives here: each `Inst` variant emits the cranelift sequence
//! that realises it, with the surrounding `BodyCx`-style state
//! threaded in as parameters from `lower_function`. Large variants
//! (Call / LoadField / StoreField) are split out into per-topic
//! submodules — they take the same long parameter list since
//! there isn't a dedicated context struct yet.

mod calls;
mod objects;

use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature};
use cranelift_frontend::{FunctionBuilder as ClifFnBuilder, Variable};
use cranelift_module::{DataId, Module};

use ilang_ast::Symbol;
use ilang_mir::{
    BinOp, FuncId, Function as MirFunction, Inst, MirConst, MirTy, Program,
    StaticSlotId, UnOp, ValueId,
};

use crate::ty::mir_to_clif;

use super::abi::{
    elem_byte_stride, elem_clif_type, extend_to_i64, ireduce_or_pass,
    reduce_from_i64, struct_chunks_with_max, struct_hfa, struct_indirect_with_max,
    IL_BYVAL_CHUNK_MAX,
};
use super::binop_cast::{lower_binop, lower_cast};
use super::lower_term_const::lower_const;
use super::print_emit::emit_panic_if;
use super::print_kind::{
    kind_tag_of, print_kind_id, KIND_ARRAY, KIND_CLOSURE, KIND_ENUM, KIND_MAP, KIND_NONE,
    KIND_OBJECT, KIND_OPTIONAL, KIND_PROMISE, KIND_STR, KIND_TUPLE,
};
use super::{
    emit_is_subclass, CompileError, FmtIds, MapIds, PanicAux, PrintIds, PrintLits, PromiseIds, StrIds,
    OBJECT_HEADER_BYTES,
};

/// Inline byte-wise copy of `total` bytes from `src` to `dst_addr`.
/// Mirrors the pattern used in `objects.rs` for CRepr struct copies —
/// it avoids depending on the JIT's `memcpy` libcall resolution
/// (which can race with how mir-codegen declares its own symbols).
fn crepr_struct_copy(fb: &mut ClifFnBuilder, src: Value, dst_addr: Value, total: i64) {
    let mut copied = 0i64;
    while copied + 8 <= total {
        let v = fb.ins().load(types::I64, MemFlags::trusted(), src, copied as i32);
        fb.ins().store(MemFlags::trusted(), v, dst_addr, copied as i32);
        copied += 8;
    }
    while copied + 4 <= total {
        let v = fb.ins().load(types::I32, MemFlags::trusted(), src, copied as i32);
        fb.ins().store(MemFlags::trusted(), v, dst_addr, copied as i32);
        copied += 4;
    }
    while copied + 2 <= total {
        let v = fb.ins().load(types::I16, MemFlags::trusted(), src, copied as i32);
        fb.ins().store(MemFlags::trusted(), v, dst_addr, copied as i32);
        copied += 2;
    }
    while copied < total {
        let v = fb.ins().load(types::I8, MemFlags::trusted(), src, copied as i32);
        fb.ins().store(MemFlags::trusted(), v, dst_addr, copied as i32);
        copied += 1;
    }
}

pub(super) fn lower_inst<M: Module>(
    fb: &mut ClifFnBuilder,
    inst: &Inst,
    vmap: &mut HashMap<ValueId, Value>,
    func: &MirFunction,
    fn_ids: &HashMap<FuncId, cranelift_module::FuncId>,
    extern_alias_fn_ids: &std::collections::HashSet<FuncId>,
    builtin_ids: &HashMap<String, (cranelift_module::FuncId, Signature)>,
    static_data: &HashMap<StaticSlotId, DataId>,
    string_data: &HashMap<Symbol, DataId>,
    alloc_id: cranelift_module::FuncId,
    map_ids: MapIds,
    promise_ids: PromiseIds,
    str_ids: StrIds,
    print_ids: PrintIds,
    fmt_ids: FmtIds,
    panic_aux: PanicAux,
    print_lits: PrintLits,
    module: &mut M,
    locals: &[Variable],
    local_slots: &[Option<cranelift_codegen::ir::StackSlot>],
    prog: &Program,
    env_value: Value,
    class_global: &[u32],
    enum_global: &[u32],
    class_struct_global: &[i64],
    stack_local: &std::collections::HashSet<ValueId>,
) -> Result<(), CompileError> {
    match inst {
        Inst::Const { dst, value } => {
            let ty = func.ty_of(*dst);
            if matches!(ty, MirTy::Unit) || matches!(value, MirConst::Unit) {
                return Ok(());
            }
            // String consts go through Cranelift `symbol_value` to get
            // the data symbol's runtime address.
            if let MirConst::Str(s) = value {
                let did = *string_data.get(s).ok_or_else(|| {
                    CompileError::Other(format!("missing string data for {:?}", s.as_str()))
                })?;
                let gv = module.declare_data_in_func(did, fb.func);
                let base = fb.ins().symbol_value(types::I64, gv);
                // The user-visible string pointer skips the 8-byte
                // length prefix (see string_data layout above).
                let off = fb.ins().iconst(types::I64, 8);
                let v = fb.ins().iadd(base, off);
                vmap.insert(*dst, v);
                return Ok(());
            }
            let cv = lower_const(fb, value, ty)?;
            vmap.insert(*dst, cv);
        }
        Inst::BinOp { dst, op, lhs, rhs } => {
            let lv = vmap[lhs];
            let rv = vmap[rhs];
            // Runtime div/0 / mod/0 check on int division.
            if matches!(
                op,
                BinOp::IDivS | BinOp::IDivU | BinOp::IRemS | BinOp::IRemU
            ) {
                let rv_ty = fb.func.dfg.value_type(rv);
                let zero = fb.ins().iconst(rv_ty, 0);
                let is_zero = fb.ins().icmp(IntCC::Equal, rv, zero);
                let msg = if matches!(op, BinOp::IRemS | BinOp::IRemU) {
                    panic_aux.msg_mod
                } else {
                    panic_aux.msg_div
                };
                emit_panic_if(fb, module, panic_aux.fn_id, msg, is_zero);
            }
            let v = match op {
                BinOp::StrConcat => {
                    let r = module.declare_func_in_func(str_ids.concat, fb.func);
                    let call = fb.ins().call(r, &[lv, rv]);
                    fb.inst_results(call)[0]
                }
                BinOp::StrConcatInplace => {
                    let r = module.declare_func_in_func(str_ids.concat_inplace, fb.func);
                    let call = fb.ins().call(r, &[lv, rv]);
                    fb.inst_results(call)[0]
                }
                BinOp::StrEq => {
                    let r = module.declare_func_in_func(str_ids.eq, fb.func);
                    let call = fb.ins().call(r, &[lv, rv]);
                    let raw = fb.inst_results(call)[0];
                    fb.ins().ireduce(types::I8, raw)
                }
                BinOp::StrNe => {
                    let r = module.declare_func_in_func(str_ids.eq, fb.func);
                    let call = fb.ins().call(r, &[lv, rv]);
                    let raw = fb.inst_results(call)[0];
                    let lo = fb.ins().ireduce(types::I8, raw);
                    let one = fb.ins().iconst(types::I8, 1);
                    fb.ins().bxor(lo, one)
                }
                _ => lower_binop(fb, *op, lv, rv),
            };
            vmap.insert(*dst, v);
        }
        Inst::UnOp { dst, op, src } => {
            let sv = vmap[src];
            let v = match op {
                UnOp::INeg => fb.ins().ineg(sv),
                UnOp::FNeg => fb.ins().fneg(sv),
                UnOp::Not => fb.ins().bnot(sv),
                UnOp::BoolNot => {
                    let zero = fb.ins().iconst(types::I8, 0);
                    fb.ins().icmp(IntCC::Equal, sv, zero)
                }
            };
            vmap.insert(*dst, v);
        }
        Inst::Cast { dst, kind, src } => {
            let sv = vmap[src];
            let dst_ty = func.ty_of(*dst);
            let src_ty = func.ty_of(*src);
            let v = lower_cast(fb, *kind, sv, dst_ty, src_ty)?;
            vmap.insert(*dst, v);
        }
        Inst::Call { dst, callee, args } => {
            calls::lower_call(
                fb, dst, callee, args, vmap, func, fn_ids, extern_alias_fn_ids,
                builtin_ids,
                static_data, string_data, alloc_id, map_ids, promise_ids, str_ids,
                print_ids, fmt_ids, panic_aux, print_lits, module, locals, prog,
                env_value, class_global, enum_global,
                class_struct_global, stack_local,
            )?;
        }
        Inst::VirtCall { dst, recv, slot, args } => {
            // Load class_id from object header, dispatch via the
            // host runtime helper, then call_indirect.
            //
            // The call_indirect signature here MUST match the
            // callee's `clif_signature_for` shape exactly, including
            // the by-value expansion for CRepr struct params /
            // returns (chunks / HFA / sret). All class methods are
            // `FunctionKind::Local`, so they always use the ilang
            // ABI chunk cap (`IL_BYVAL_CHUNK_MAX`). A pre-fix
            // version of this arm built the signature naively from
            // the args' clif types, which lined up only when no
            // CRepr params were involved — once SDL's `Renderer.copy
            // (srcrect: Rect, dstrect: Rect)` got chunked on the
            // callee side, the virt-dispatch call site still passed
            // single i64 pointers and the chunk slots received
            // garbage. Hence broken text rendering in breakout.
            let recv_v = vmap[recv];
            let cid_v = fb.ins().load(types::I64, MemFlags::trusted(), recv_v, 0);
            let slot_v = fb.ins().iconst(types::I64, slot.0 as i64);
            let dispatch_ref = module.declare_func_in_func(str_ids.virt_dispatch, fb.func);
            let lookup = fb.ins().call(dispatch_ref, &[cid_v, slot_v]);
            let fn_ptr = fb.inst_results(lookup)[0];

            let chunk_max = IL_BYVAL_CHUNK_MAX;
            let mut clif_sig = module.make_signature();
            let mut arg_vs: Vec<Value> = Vec::with_capacity(args.len() + 2);

            // Sret prefix on the return side: a CRepr return that
            // overflows the chunk cap needs the caller to allocate
            // a destination buffer and hand its pointer as the
            // hidden first arg.
            let dst_ty_mir = dst.map(|d| func.ty_of(d).clone());
            let sret_dst = if let Some(t) = &dst_ty_mir {
                if let Some(c_size) = struct_indirect_with_max(t, prog, chunk_max) {
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
            let closure = vmap[callee];
            let fn_ptr = fb.ins().load(types::I64, MemFlags::trusted(), closure, 0);
            // Build an indirect signature: user params + trailing env.
            let mut clif_sig = module.make_signature();
            for p in sig.params.iter() {
                if let Some(ct) = mir_to_clif(p) {
                    clif_sig.params.push(AbiParam::new(ct));
                }
            }
            clif_sig.params.push(AbiParam::new(types::I64));
            if !matches!(sig.ret, MirTy::Unit) {
                if let Some(ct) = mir_to_clif(&sig.ret) {
                    clif_sig.returns.push(AbiParam::new(ct));
                }
            }
            let sig_ref = fb.import_signature(clif_sig);
            let mut arg_vs: Vec<Value> = args.iter().map(|a| vmap[a]).collect();
            arg_vs.push(closure); // env_ptr = closure block ptr
            let inst_ref = fb.ins().call_indirect(sig_ref, fn_ptr, &arg_vs);
            if let Some(d) = dst {
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
            // it — the lowering site supplies just the post-this
            // arguments; we prepend here.
            let recv_v = vmap[recv];
            let vt = fb.ins().load(types::I64, MemFlags::trusted(), recv_v, 0);
            let slot_off = (*slot as i32) * 8;
            let fp = fb.ins().load(types::I64, MemFlags::trusted(), vt, slot_off);
            let mut clif_sig = module.make_signature();
            // Inherit the enclosing fn's call convention so stack-arg
            // / shadow-store placement matches what the COM method
            // expects (WindowsFastcall on x64 MSVC, SystemV on
            // Linux). The module default is sometimes the platform
            // C calling convention, but explicitly pinning it makes
            // the >4-arg case lay out the stack correctly.
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
        Inst::MakeClosure { dst, func: fid, captures } => {
            let cid = *fn_ids.get(fid).ok_or_else(|| {
                CompileError::Other(format!("missing fn id #{}", fid.0))
            })?;
            let local_ref = module.declare_func_in_func(cid, fb.func);
            let n_caps = captures.len() as i64;
            // Layout: [fn_ptr @ 0 | rc @ 8 | capture_0 @ 16 | ...]
            let bytes = fb.ins().iconst(types::I64, (2 + n_caps) * 8);
            let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
            let call = fb.ins().call(alloc_ref, &[bytes]);
            let ptr = fb.inst_results(call)[0];
            let fn_addr = fb.ins().func_addr(types::I64, local_ref);
            fb.ins().store(MemFlags::trusted(), fn_addr, ptr, 0);
            let one = fb.ins().iconst(types::I64, 1);
            fb.ins().store(MemFlags::trusted(), one, ptr, 8);
            for (i, c) in captures.iter().enumerate() {
                let v_ext = extend_to_i64(fb, vmap[c]);
                fb.ins().store(
                    MemFlags::trusted(),
                    v_ext,
                    ptr,
                    16 + (i as i32) * 8,
                );
            }
            vmap.insert(*dst, ptr);
        }
        Inst::FuncAddr { dst, func: fid } => {
            // Bare 8-byte function code address — no closure box.
            // Stored as-is in `@extern(C)` struct fields of `fn(...)`
            // type so C code sees a real `T (*)(...)`.
            let cid = *fn_ids.get(fid).ok_or_else(|| {
                CompileError::Other(format!("missing fn id #{}", fid.0))
            })?;
            let local_ref = module.declare_func_in_func(cid, fb.func);
            let addr = fb.ins().func_addr(types::I64, local_ref);
            vmap.insert(*dst, addr);
        }
        Inst::LoadCapture { dst, idx } => {
            // Captures live at `env + 16 + idx*8`; env is the closure
            // block pointer (the trailing hidden param).
            let off = 16 + (*idx as i32) * 8;
            let raw = fb.ins().load(types::I64, MemFlags::trusted(), env_value, off);
            let dst_ty = func.ty_of(*dst).clone();
            let v = reduce_from_i64(fb, &dst_ty, raw);
            vmap.insert(*dst, v);
        }
        // ARC operations are stubbed in M1: refcount machinery
        // arrives once the runtime is wired. Treating them as no-ops
        // means programs leak heap allocations until then, which is
        // acceptable for short-running test programs.
        Inst::Release { value } => {
            // Stack-promoted objects live in the function frame; the
            // stack unwinder reclaims them automatically at return.
            // Calling __release_object on a stack pointer would
            // attempt a heap free → crash. Just skip.
            if stack_local.contains(value) {
                return Ok(());
            }
            let aty = func.ty_of(*value).clone();
            match &aty {
                MirTy::Object(cid) => {
                    let layout = &prog.classes[cid.0 as usize];
                    // Mirror the Retain side: a `@com interface`
                    // handle is a foreign COM pointer — releasing
                    // it via `__release_object` would scribble at
                    // `com_ptr + 8`, inside whatever real data
                    // structure D3D12 / etc. parks there. Lifetime
                    // is the user's responsibility through
                    // `IUnknown::Release`. `@handle pub struct` is
                    // the same shape, same rule.
                    if layout.is_com_interface || layout.is_handle {
                        return Ok(());
                    }
                    if matches!(
                        layout.repr,
                        ilang_mir::ClassRepr::CRepr
                            | ilang_mir::ClassRepr::CPacked
                            | ilang_mir::ClassRepr::CUnion
                    ) {
                        // CRepr struct: no rc header, free the
                        // backing buffer directly. The lower side
                        // only emits this Release for Locals
                        // tagged in `crepr_owned_locals` — i.e.
                        // values that came from a fresh NewObject
                        // (or an aggregate-literal desugar that
                        // owns its temp), never a `let p =
                        // r.origin` borrow.
                        let av = vmap[value];
                        let sz = layout.c_size.max(1);
                        let sz_v = fb.ins().iconst(types::I64, sz);
                        let r = module.declare_func_in_func(panic_aux.mir_free, fb.func);
                        fb.ins().call(r, &[av, sz_v]);
                        return Ok(());
                    }
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.release_obj, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Fn(_) => {
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.release_closure, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Array { len, .. } => {
                    if len.is_some() {
                        return Ok(());
                    }
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.release_array, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Optional(_) => {
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.release_optional, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Tuple(_) => {
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.release_tuple, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Map { .. } => {
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.release_map, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Promise(_) => {
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.release_promise, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Str => {
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.release_string, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Enum(_) => {
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.release_enum, fb.func);
                    fb.ins().call(r, &[av]);
                }
                _ => {}
            }
        }
        Inst::Retain { value } => {
            // Same rationale as the matching `Release` branch: a
            // stack-promoted object has no rc to bump.
            if stack_local.contains(value) {
                return Ok(());
            }
            let aty = func.ty_of(*value).clone();
            match &aty {
                MirTy::Object(cid) => {
                    let layout = &prog.classes[cid.0 as usize];
                    if matches!(
                        layout.repr,
                        ilang_mir::ClassRepr::CRepr
                            | ilang_mir::ClassRepr::CPacked
                            | ilang_mir::ClassRepr::CUnion
                    ) {
                        return Ok(());
                    }
                    // `@com interface` handles carry no ilang rc —
                    // `__retain_object` would atomic-increment at
                    // `com_ptr + 8`, which on a real COM resource is
                    // private data the foreign runtime owns. Skip;
                    // user code uses `IUnknown::AddRef` for the COM
                    // lifetime contract. Same applies to
                    // `@handle pub struct H {}` — Win32-style raw
                    // pointer handle, no rc plumbing.
                    if layout.is_com_interface || layout.is_handle {
                        return Ok(());
                    }
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.retain_obj, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Fn(_) => {
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.retain_closure, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Array { len, .. } => {
                    if len.is_some() {
                        return Ok(());
                    }
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.retain_array, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Optional(_) => {
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.retain_optional, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Tuple(_) => {
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.retain_tuple, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Map { .. } => {
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.retain_map, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Promise(_) => {
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.retain_promise, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Str => {
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.retain_string, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Enum(_) => {
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.retain_enum, fb.func);
                    fb.ins().call(r, &[av]);
                }
                _ => {}
            }
        }
        Inst::WeakRetain { .. }
        | Inst::WeakRelease { .. } => {}
        Inst::TypeOf { dst, value } => {
            // Return the dynamic class id (i64) — used as an opaque
            // `Type` handle. Full `Type` API arrives with the runtime.
            let p = vmap[value];
            let cid = fb.ins().load(types::I64, MemFlags::trusted(), p, 0);
            vmap.insert(*dst, cid);
        }
        Inst::IsInstance { dst, value, class } => {
            let p = vmap[value];
            let cid = fb.ins().load(types::I64, MemFlags::trusted(), p, 0);
            let v = emit_is_subclass(fb, cid, *class, prog, class_global);
            vmap.insert(*dst, v);
        }
        Inst::DowncastOrNone { dst, value, class } => {
            // `value as? Class` → some(value) if dynamic class is
            // a subtype of `class`, else none. Optional<Object> is
            // boxed: we emit NewOptional on the some-branch, 0 on the
            // none-branch, and merge through a block-arg.
            let p = vmap[value];
            let cid = fb.ins().load(types::I64, MemFlags::trusted(), p, 0);
            let cond = emit_is_subclass(fb, cid, *class, prog, class_global);

            let some_blk = fb.create_block();
            let none_blk = fb.create_block();
            let cont_blk = fb.create_block();
            let result = fb.append_block_param(cont_blk, types::I64);

            fb.ins().brif(cond, some_blk, &[], none_blk, &[]);

            fb.switch_to_block(some_blk);
            fb.seal_block(some_blk);
            // Allocate one i64 cell containing the value.
            let bytes = fb.ins().iconst(types::I64, 8);
            let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
            let call = fb.ins().call(alloc_ref, &[bytes]);
            let ptr = fb.inst_results(call)[0];
            fb.ins().store(MemFlags::trusted(), p, ptr, 0);
            fb.ins().jump(cont_blk, [cranelift_codegen::ir::BlockArg::from(ptr)].iter());

            fb.switch_to_block(none_blk);
            fb.seal_block(none_blk);
            let zero = fb.ins().iconst(types::I64, 0);
            fb.ins().jump(cont_blk, [cranelift_codegen::ir::BlockArg::from(zero)].iter());

            fb.switch_to_block(cont_blk);
            fb.seal_block(cont_blk);
            vmap.insert(*dst, result);
        }
        Inst::WeakUpgrade { dst, weak } => {
            // Weak refs share storage with the strong rep. Upgrade
            // returns `some(target)` only when the target's strong rc
            // is still positive; otherwise `none`. The Optional cell
            // is a 3-cell heap [value | rc | kind_tag=Object].
            let p = vmap[weak];
            let zero = fb.ins().iconst(types::I64, 0);
            let none_blk = fb.create_block();
            let some_blk = fb.create_block();
            let cont = fb.create_block();
            fb.append_block_param(cont, types::I64);

            let p_nz = fb.ins().icmp(IntCC::NotEqual, p, zero);
            fb.ins().brif(p_nz, some_blk, &[], none_blk, &[]);

            // Test target rc.
            fb.switch_to_block(some_blk);
            fb.seal_block(some_blk);
            let rc = fb.ins().load(types::I64, MemFlags::trusted(), p, 8);
            let alive = fb.ins().icmp_imm(IntCC::SignedGreaterThan, rc, 0);
            let alloc_blk = fb.create_block();
            fb.ins().brif(alive, alloc_blk, &[], none_blk, &[]);

            // alive: bump strong rc (caller now owns +1) and box into
            // a fresh Optional cell.
            fb.switch_to_block(alloc_blk);
            fb.seal_block(alloc_blk);
            let one = fb.ins().iconst(types::I64, 1);
            let new_rc = fb.ins().iadd(rc, one);
            fb.ins().store(MemFlags::trusted(), new_rc, p, 8);
            let bytes = fb.ins().iconst(types::I64, 24);
            let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
            let call = fb.ins().call(alloc_ref, &[bytes]);
            let cell = fb.inst_results(call)[0];
            fb.ins().store(MemFlags::trusted(), p, cell, 0);
            fb.ins().store(MemFlags::trusted(), one, cell, 8);
            let kind = fb.ins().iconst(types::I64, 1); // PrintKind::Object cascade
            fb.ins().store(MemFlags::trusted(), kind, cell, 16);
            fb.ins().jump(cont, [cell.into()].iter());

            fb.switch_to_block(none_blk);
            fb.seal_block(none_blk);
            fb.ins().jump(cont, [zero.into()].iter());

            fb.switch_to_block(cont);
            fb.seal_block(cont);
            let v = fb.block_params(cont)[0];
            vmap.insert(*dst, v);
        }
        Inst::DefLocal { local, value } => {
            let v = vmap[value];
            if std::env::var("ILANG_DEBUG_DEFLOCAL").is_ok() {
                let want = func.local_tys[local.0 as usize].clone();
                let got = fb.func.dfg.value_type(v);
                eprintln!(
                    "[deflocal] fn={} local#{} declared={want} clif_val_ty={got}",
                    func.name.as_str(), local.0
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
        Inst::NewObject { dst, class, init_args, init } => {
            let layout = &prog.classes[class.0 as usize];
            // `@extern(C) struct` lives flat with no header / rc:
            // alloc exactly c_size bytes (zero-init by host_mir_alloc)
            // and bind that pointer. No init / deinit / vtable.
            if matches!(
                layout.repr,
                ilang_mir::ClassRepr::CRepr
                    | ilang_mir::ClassRepr::CPacked
                    | ilang_mir::ClassRepr::CUnion
            ) {
                // CRepr struct alloc. Two paths:
                //
                // 1. Stack promotion (`stack_local.contains(dst)`):
                //    escape analysis cleared this allocation, so back
                //    it with a function-local Cranelift StackSlot of
                //    `c_size` bytes instead of going through
                //    `__mir_alloc`. Field offsets are computed by
                //    LoadField / StoreField from `c_field_offsets`
                //    against whatever base pointer we hand back,
                //    which works identically for heap and stack
                //    memory. The flex-array-tail form
                //    (`new Packet(n)`) needs a dynamic size, so it's
                //    not eligible — fall through to the heap path.
                //
                // 2. Heap (default): one call to `__mir_alloc` for
                //    `c_size` bytes (or `c_size + n*flex_elem_size`
                //    for the FAM form).
                let stack_ok =
                    stack_local.contains(dst) && layout.flex_elem_size == 0;
                let ptr = if stack_ok {
                    let slot_size = (layout.c_size.max(1)) as u32;
                    let slot = fb.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        slot_size,
                        // log2 of alignment. 8-byte alignment covers
                        // every primitive a top-level struct field
                        // can hold (i64 / f64 / pointer); over-
                        // aligning is cheap since the slot's purely
                        // local.
                        3,
                    ));
                    let p = fb.ins().stack_addr(types::I64, slot, 0);
                    // Zero the slot to mirror `__mir_alloc`'s
                    // zero-init contract — primitive field reads
                    // before the first write must see 0 instead of
                    // stack garbage. Whole-slot 8-byte stores are
                    // safe here because `c_size` rounds up to the
                    // largest field's alignment in `class_signature`.
                    let zero = fb.ins().iconst(types::I64, 0);
                    let mut off: i32 = 0;
                    while (off as i64) < slot_size as i64 {
                        fb.ins().store(MemFlags::trusted(), zero, p, off);
                        off += 8;
                    }
                    p
                } else {
                    let size_v = if layout.flex_elem_size > 0 && !init_args.is_empty() {
                        let n_v = vmap[&init_args[0]];
                        let n_i64 = extend_to_i64(fb, n_v);
                        let elem_v = fb.ins().iconst(types::I64, layout.flex_elem_size);
                        let extra = fb.ins().imul(n_i64, elem_v);
                        let base = fb.ins().iconst(types::I64, layout.c_size.max(0));
                        fb.ins().iadd(base, extra)
                    } else {
                        fb.ins().iconst(types::I64, layout.c_size.max(1))
                    };
                    let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
                    let alloc_call = fb.ins().call(alloc_ref, &[size_v]);
                    fb.inst_results(alloc_call)[0]
                };
                vmap.insert(*dst, ptr);
                return Ok(());
            }
            let n_fields = layout.fields.len() as i64;
            let total_bytes = OBJECT_HEADER_BYTES as i64 + n_fields * 8;
            // Stack-promotion fast path: escape analysis has cleared
            // this `dst`, so allocate a cranelift StackSlot inside
            // the current function frame instead of going through
            // __mir_alloc. Field offsets and LoadField / StoreField
            // / VirtCall layouts stay identical (header + n*8). The
            // matching `Retain` / `Release` calls are no-op'd below
            // so the stack memory's lifetime is the function frame's.
            let ptr = if stack_local.contains(dst) {
                let slot = fb.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    total_bytes as u32,
                    3,
                ));
                let p = fb.ins().stack_addr(types::I64, slot, 0);
                // Zero the slot's bytes — heap alloc zeros via
                // __mir_alloc; we keep the same invariant so any
                // primitive field read before its first write sees
                // 0 instead of stack garbage.
                let zero = fb.ins().iconst(types::I64, 0);
                let mut off = 0;
                while off < total_bytes {
                    fb.ins().store(MemFlags::trusted(), zero, p, off as i32);
                    off += 8;
                }
                p
            } else {
                let size = fb.ins().iconst(types::I64, total_bytes);
                let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
                let alloc_call = fb.ins().call(alloc_ref, &[size]);
                fb.inst_results(alloc_call)[0]
            };
            // Store the GLOBAL class id at obj+0 — release_object,
            // host_print_object and __virt_dispatch all key off this.
            let cid_v = fb.ins().iconst(types::I64, class_global[class.0 as usize] as i64);
            fb.ins().store(MemFlags::trusted(), cid_v, ptr, 0);
            let one = fb.ins().iconst(types::I64, 1);
            fb.ins().store(MemFlags::trusted(), one, ptr, 8);

            if init.0 != u32::MAX {
                let cid = *fn_ids.get(init).ok_or_else(|| {
                    CompileError::Other(format!("missing init fn id #{}", init.0))
                })?;
                let local_ref = module.declare_func_in_func(cid, fb.func);
                let mut args: Vec<Value> = Vec::with_capacity(init_args.len() + 2);
                args.push(ptr);
                for a in init_args.iter() {
                    args.push(vmap[a]);
                }
                // Trailing env-ptr (unused by init).
                let zero = fb.ins().iconst(types::I64, 0);
                args.push(zero);
                let call_inst = fb.ins().call(local_ref, &args);
                // init returns `this`; use it (in case the runtime
                // ever wraps the receiver).
                let returned = fb.inst_results(call_inst).first().copied();
                let result = returned.unwrap_or(ptr);
                vmap.insert(*dst, result);
            } else {
                vmap.insert(*dst, ptr);
            }
        }
        Inst::NewArray { dst, elem, items } => {
            // Detect `@extern(C) struct` elements: those need to be
            // copied inline so the resulting buffer matches the C
            // layout `Elem buf[N]` byte-for-byte (rather than
            // degenerating to an array of heap pointers).
            let crepr_struct_elem_size: Option<i64> = if let MirTy::Object(cid) = elem {
                let cls = &prog.classes[cid.0 as usize];
                use ilang_mir::ClassRepr;
                if matches!(
                    cls.repr,
                    ClassRepr::CRepr | ClassRepr::CPacked | ClassRepr::CUnion
                ) && cls.c_size > 0
                {
                    Some(cls.c_size)
                } else {
                    None
                }
            } else {
                None
            };
            // Inline fixed-length output (when the dst MirTy carries
            // `len: Some(n)`): allocate `n*stride` bytes with no
            // header, store elements directly at `data + i*stride`.
            // This keeps the layout consistent with array fields of
            // `@extern(C)` structs that LoadField returns as inline
            // addresses.
            let dst_ty = func.ty_of(*dst).clone();
            if let MirTy::Array { len: Some(_), .. } = &dst_ty {
                let stride_bytes = crepr_struct_elem_size.unwrap_or_else(|| elem_byte_stride(elem));
                let n = items.len() as i64;
                let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
                let bytes = fb.ins().iconst(types::I64, n.max(1) * stride_bytes);
                let call = fb.ins().call(alloc_ref, &[bytes]);
                let ptr = fb.inst_results(call)[0];
                let elem_clif_opt = elem_clif_type(elem);
                for (i, it) in items.iter().enumerate() {
                    let raw = vmap[it];
                    let off = (i as i32) * (stride_bytes as i32);
                    if let Some(total) = crepr_struct_elem_size {
                        // Inline byte-wise copy of the struct's bytes
                        // from the source heap slot to data+off.
                        let dst_addr = if off == 0 {
                            ptr
                        } else {
                            let off_v = fb.ins().iconst(types::I64, off as i64);
                            fb.ins().iadd(ptr, off_v)
                        };
                        crepr_struct_copy(fb, raw, dst_addr, total);
                    } else if let Some(elem_ct) = elem_clif_opt {
                        let truncated = ireduce_or_pass(fb, raw, elem_ct);
                        fb.ins().store(MemFlags::trusted(), truncated, ptr, off);
                    } else {
                        let v_ext = extend_to_i64(fb, raw);
                        fb.ins().store(MemFlags::trusted(), v_ext, ptr, off);
                    }
                }
                vmap.insert(*dst, ptr);
                return Ok(());
            }
            // Layout: 6-i64 header [len | cap | data_ptr | rc | kind_tag | stride]
            // + separately-allocated `stride×capacity` buffer. stride is
            // 1/2/4/8 picked from `elem` so `u8[]` / `u16[]` / `u32[]`
            // pack tightly enough for native memcpy/memset to land on
            // the right slots. For CRepr struct elements the stride
            // is the struct's `c_size` and each slot stores the
            // struct's bytes inline.
            let stride_bytes = crepr_struct_elem_size.unwrap_or_else(|| elem_byte_stride(elem));
            let n = items.len() as i64;
            let header_bytes = fb.ins().iconst(types::I64, 48);
            let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
            let call = fb.ins().call(alloc_ref, &[header_bytes]);
            let ptr = fb.inst_results(call)[0];
            let data_bytes = fb.ins().iconst(types::I64, n.max(1) * stride_bytes);
            let dcall = fb.ins().call(alloc_ref, &[data_bytes]);
            let data_ptr = fb.inst_results(dcall)[0];

            let len_v = fb.ins().iconst(types::I64, n);
            fb.ins().store(MemFlags::trusted(), len_v, ptr, 0);
            fb.ins().store(MemFlags::trusted(), len_v, ptr, 8);
            fb.ins().store(MemFlags::trusted(), data_ptr, ptr, 16);
            let one = fb.ins().iconst(types::I64, 1);
            fb.ins().store(MemFlags::trusted(), one, ptr, 24);
            let tag = kind_tag_of(elem, &prog.classes);
            let tag_v = fb.ins().iconst(types::I64, tag);
            fb.ins().store(MemFlags::trusted(), tag_v, ptr, 32);
            let stride_v = fb.ins().iconst(types::I64, stride_bytes);
            fb.ins().store(MemFlags::trusted(), stride_v, ptr, 40);
            let elem_clif_opt = elem_clif_type(elem);
            for (i, it) in items.iter().enumerate() {
                let raw = vmap[it];
                let off = (i as i32) * (stride_bytes as i32);
                if let Some(total) = crepr_struct_elem_size {
                    let dst_addr = if off == 0 {
                        data_ptr
                    } else {
                        let off_v = fb.ins().iconst(types::I64, off as i64);
                        fb.ins().iadd(data_ptr, off_v)
                    };
                    crepr_struct_copy(fb, raw, dst_addr, total);
                } else if let Some(elem_ct) = elem_clif_opt {
                    let truncated = ireduce_or_pass(fb, raw, elem_ct);
                    fb.ins().store(MemFlags::trusted(), truncated, data_ptr, off);
                } else {
                    let v_ext = extend_to_i64(fb, raw);
                    fb.ins().store(MemFlags::trusted(), v_ext, data_ptr, off);
                }
            }
            vmap.insert(*dst, ptr);
        }
        Inst::NewArrayEmpty { dst, elem, fixed_len } => {
            let stride_bytes = elem_byte_stride(elem);
            let n = fixed_len.unwrap_or(0) as i64;
            let header_bytes = fb.ins().iconst(types::I64, 48);
            let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
            let call = fb.ins().call(alloc_ref, &[header_bytes]);
            let ptr = fb.inst_results(call)[0];
            let cap = n.max(4);
            let data_bytes = fb.ins().iconst(types::I64, cap * stride_bytes);
            let dcall = fb.ins().call(alloc_ref, &[data_bytes]);
            let data_ptr = fb.inst_results(dcall)[0];
            let len_v = fb.ins().iconst(types::I64, n);
            let cap_v = fb.ins().iconst(types::I64, cap);
            fb.ins().store(MemFlags::trusted(), len_v, ptr, 0);
            fb.ins().store(MemFlags::trusted(), cap_v, ptr, 8);
            fb.ins().store(MemFlags::trusted(), data_ptr, ptr, 16);
            let one = fb.ins().iconst(types::I64, 1);
            fb.ins().store(MemFlags::trusted(), one, ptr, 24);
            let tag = kind_tag_of(elem, &prog.classes);
            let tag_v = fb.ins().iconst(types::I64, tag);
            fb.ins().store(MemFlags::trusted(), tag_v, ptr, 32);
            let stride_v = fb.ins().iconst(types::I64, stride_bytes);
            fb.ins().store(MemFlags::trusted(), stride_v, ptr, 40);
            vmap.insert(*dst, ptr);
        }
        Inst::NewSimd { dst, lanes } => {
            // Pack `lanes` scalar values into a cranelift vector via
            // a temporary stack slot: store each lane at its byte
            // offset, then issue one vector load. Avoids
            // `scalar_to_vector` whose arm64 ISLE lowering is still
            // a TODO for some lane widths (e.g. `f32x2`), and keeps
            // the lowering uniform across all SIMD widths.
            let dst_ty = func.ty_of(*dst).clone();
            let cl_vec_ty = mir_to_clif(&dst_ty).ok_or(
                CompileError::Unsupported("SIMD type with no cranelift mapping"),
            )?;
            let (lane_elem, lane_count) = match &dst_ty {
                MirTy::Simd { elem, lanes: n } => (*elem, *n as i64),
                _ => return Err(CompileError::Unsupported("NewSimd on non-SIMD type")),
            };
            let lane_bytes = lane_elem.lane_bytes();
            let total = (lane_bytes * lane_count) as u32;
            let slot = fb.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                total,
                0,
            ));
            let lane_scalar_ct = elem_clif_type(&lane_elem.as_scalar_mir())
                .ok_or(CompileError::Unsupported("SIMD lane has no clif scalar type"))?;
            for (i, lane) in lanes.iter().enumerate() {
                let off = (i as i64 * lane_bytes) as i32;
                let raw = vmap[lane];
                let stored = ireduce_or_pass(fb, raw, lane_scalar_ct);
                fb.ins().stack_store(stored, slot, off);
            }
            let v = fb.ins().stack_load(cl_vec_ty, slot, 0);
            vmap.insert(*dst, v);
        }
        Inst::ArrayLen { dst, arr } => {
            let arr_ty = func.ty_of(*arr).clone();
            let v = if let MirTy::Array { len: Some(n), .. } = &arr_ty {
                fb.ins().iconst(types::I64, *n as i64)
            } else {
                let p = vmap[arr];
                fb.ins().load(types::I64, MemFlags::trusted(), p, 0)
            };
            vmap.insert(*dst, v);
        }
        Inst::ArrayLoad { dst, arr, idx } => {
            let p = vmap[arr];
            let i_raw = vmap[idx];
            // Index may come in as a narrower int (i32 / u32 / etc.)
            // when the source code uses an int-typed counter. The
            // OOB check + offset arithmetic below all run on i64,
            // so widen up-front rather than threading a sign-cross
            // cast at every consumer.
            let i = extend_to_i64(fb, i_raw);
            // Inline fixed-size array (`u8[4]` field of an @extern(C)
            // struct, etc) — base ptr is the start of the elements,
            // no header. Use the static elem stride from the type.
            // For CRepr-struct elements the stride is the class's
            // `c_size` (not the 8-byte default in `elem_byte_stride`)
            // and the element value *is* the inline address — the
            // same convention LoadField uses for nested CRepr fields.
            let arr_ty = func.ty_of(*arr).clone();
            let crepr_elem_size: Option<i64> = if let MirTy::Array {
                elem,
                len: Some(_),
            } = &arr_ty
            {
                if let MirTy::Object(cid) = &**elem {
                    let cls = &prog.classes[cid.0 as usize];
                    use ilang_mir::ClassRepr;
                    if matches!(
                        cls.repr,
                        ClassRepr::CRepr | ClassRepr::CPacked | ClassRepr::CUnion
                    ) && cls.c_size > 0
                    {
                        Some(cls.c_size)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };
            let inline_info = match &arr_ty {
                MirTy::Array { elem, len: Some(n) } => Some((
                    crepr_elem_size.unwrap_or_else(|| elem_byte_stride(elem)),
                    *n as i64,
                )),
                _ => None,
            };
            let (data_ptr, stride) = if let Some((s, n)) = inline_info {
                let n_v = fb.ins().iconst(types::I64, n);
                let oob_lo = fb.ins().icmp_imm(IntCC::SignedLessThan, i, 0);
                let oob_hi = fb.ins().icmp(IntCC::SignedGreaterThanOrEqual, i, n_v);
                let oob = fb.ins().bor(oob_lo, oob_hi);
                emit_panic_if(fb, module, panic_aux.fn_id, panic_aux.msg_oob, oob);
                let s_v = fb.ins().iconst(types::I64, s);
                (p, s_v)
            } else {
                let len = fb.ins().load(types::I64, MemFlags::trusted(), p, 0);
                let oob_lo = fb.ins().icmp_imm(IntCC::SignedLessThan, i, 0);
                let oob_hi = fb.ins().icmp(IntCC::SignedGreaterThanOrEqual, i, len);
                let oob = fb.ins().bor(oob_lo, oob_hi);
                emit_panic_if(fb, module, panic_aux.fn_id, panic_aux.msg_oob, oob);
                let data_ptr = fb.ins().load(types::I64, MemFlags::trusted(), p, 16);
                let stride = fb.ins().load(types::I64, MemFlags::trusted(), p, 40);
                (data_ptr, stride)
            };
            let off = fb.ins().imul(i, stride);
            let addr = fb.ins().iadd(data_ptr, off);
            let dst_ty_mir = func.ty_of(*dst);
            // CRepr struct element: hand back the inline address as-is
            // (no load) so downstream LoadField sees `addr` and applies
            // its own `c_field_offsets` arithmetic.
            let v = if crepr_elem_size.is_some() {
                addr
            } else {
                match elem_clif_type(dst_ty_mir) {
                    Some(elem_ct) if elem_ct == types::I8 => {
                        fb.ins().load(types::I8, MemFlags::trusted(), addr, 0)
                    }
                    Some(elem_ct) if elem_ct == types::I16 => {
                        fb.ins().load(types::I16, MemFlags::trusted(), addr, 0)
                    }
                    Some(elem_ct) if elem_ct == types::I32 => {
                        fb.ins().load(types::I32, MemFlags::trusted(), addr, 0)
                    }
                    Some(elem_ct) if elem_ct == types::F32 => {
                        fb.ins().load(types::F32, MemFlags::trusted(), addr, 0)
                    }
                    Some(elem_ct) if elem_ct == types::F64 => {
                        fb.ins().load(types::F64, MemFlags::trusted(), addr, 0)
                    }
                    _ => {
                        let raw = fb.ins().load(types::I64, MemFlags::trusted(), addr, 0);
                        reduce_from_i64(fb, dst_ty_mir, raw)
                    }
                }
            };
            vmap.insert(*dst, v);
        }
        Inst::ArrayStore { arr, idx, value } => {
            let p = vmap[arr];
            let i_raw = vmap[idx];
            let i = extend_to_i64(fb, i_raw);
            let arr_ty = func.ty_of(*arr).clone();
            let inline_info = match &arr_ty {
                MirTy::Array { elem, len: Some(n) } => {
                    Some((elem_byte_stride(elem), *n as i64))
                }
                _ => None,
            };
            let (data_ptr, stride) = if let Some((s, n)) = inline_info {
                let n_v = fb.ins().iconst(types::I64, n);
                let oob_lo = fb.ins().icmp_imm(IntCC::SignedLessThan, i, 0);
                let oob_hi = fb.ins().icmp(IntCC::SignedGreaterThanOrEqual, i, n_v);
                let oob = fb.ins().bor(oob_lo, oob_hi);
                emit_panic_if(fb, module, panic_aux.fn_id, panic_aux.msg_oob, oob);
                let s_v = fb.ins().iconst(types::I64, s);
                (p, s_v)
            } else {
                let len = fb.ins().load(types::I64, MemFlags::trusted(), p, 0);
                let oob_lo = fb.ins().icmp_imm(IntCC::SignedLessThan, i, 0);
                let oob_hi = fb.ins().icmp(IntCC::SignedGreaterThanOrEqual, i, len);
                let oob = fb.ins().bor(oob_lo, oob_hi);
                emit_panic_if(fb, module, panic_aux.fn_id, panic_aux.msg_oob, oob);
                let data_ptr = fb.ins().load(types::I64, MemFlags::trusted(), p, 16);
                let stride = fb.ins().load(types::I64, MemFlags::trusted(), p, 40);
                (data_ptr, stride)
            };
            let off = fb.ins().imul(i, stride);
            let addr = fb.ins().iadd(data_ptr, off);
            let val_ty_mir = func.ty_of(*value);
            let raw = vmap[value];
            match elem_clif_type(val_ty_mir) {
                Some(elem_ct) if elem_ct != types::I64 => {
                    let truncated = ireduce_or_pass(fb, raw, elem_ct);
                    fb.ins().store(MemFlags::trusted(), truncated, addr, 0);
                }
                _ => {
                    let v_ext = extend_to_i64(fb, raw);
                    fb.ins().store(MemFlags::trusted(), v_ext, addr, 0);
                }
            }
        }
        Inst::NewMap { dst, key, val, entries } => {
            let new_ref = module.declare_func_in_func(map_ids.new, fb.func);
            let call = fb.ins().call(new_ref, &[]);
            let map_ptr = fb.inst_results(call)[0];
            // Tag the map's value-side runtime kind so host_map_set
            // can retain on insert and host_release_map can cascade-
            // release on drop, for any heap-typed value (Object,
            // String, Array, Tuple, Optional, Map, Closure, Enum).
            let val_kind = kind_tag_of(val, &prog.classes);
            if val_kind != KIND_NONE {
                let mark_ref =
                    module.declare_func_in_func(panic_aux.map_set_val_kind, fb.func);
                let kind_v = fb.ins().iconst(types::I64, val_kind);
                fb.ins().call(mark_ref, &[map_ptr, kind_v]);
            }
            // Tag the map with key/value print-kind ids so
            // `console.log(map)` can format entries correctly.
            let kk = fb.ins().iconst(types::I64, print_kind_id(key));
            let vk = fb.ins().iconst(types::I64, print_kind_id(val));
            let pk_ref =
                module.declare_func_in_func(panic_aux.map_set_print_kinds, fb.func);
            fb.ins().call(pk_ref, &[map_ptr, kk, vk]);
            let set_ref = module.declare_func_in_func(map_ids.set, fb.func);
            for (k, v) in entries.iter() {
                let kv = extend_to_i64(fb, vmap[k]);
                let vv = extend_to_i64(fb, vmap[v]);
                fb.ins().call(set_ref, &[map_ptr, kv, vv]);
            }
            vmap.insert(*dst, map_ptr);
        }
        Inst::MapGet { dst, map, key } => {
            let m = vmap[map];
            let k = extend_to_i64(fb, vmap[key]);
            let get_ref = module.declare_func_in_func(map_ids.get, fb.func);
            let call = fb.ins().call(get_ref, &[m, k]);
            let raw = fb.inst_results(call)[0];
            let dst_ty = func.ty_of(*dst).clone();
            let v = reduce_from_i64(fb, &dst_ty, raw);
            vmap.insert(*dst, v);
        }
        Inst::MapSet { map, key, value } => {
            let m = vmap[map];
            let k = extend_to_i64(fb, vmap[key]);
            let v = extend_to_i64(fb, vmap[value]);
            let set_ref = module.declare_func_in_func(map_ids.set, fb.func);
            fb.ins().call(set_ref, &[m, k, v]);
        }
        Inst::NewEnum { dst, enum_id, variant, payload } => {
            let layout = &prog.enums[enum_id.0 as usize];
            let v = &layout.variants[variant.0 as usize];
            let n_payload = match &v.payload {
                ilang_mir::VariantPayload::Unit => 0i64,
                ilang_mir::VariantPayload::Tuple(ts) => ts.len() as i64,
                ilang_mir::VariantPayload::Struct(fs) => fs.len() as i64,
            };
            // Unit-variant fast path: every `EnumName.unitVariant`
            // expression is value-equivalent (just a tag), so dispatch
            // through a process-wide cache keyed by
            // (global_enum_id, discriminant). Avoids the 8-byte
            // alloc-per-call leak for things like
            // `gamepad.isPressed(sdl.Button.a)` in a 60fps loop —
            // those fired ~840×/sec before this change.
            if n_payload == 0 {
                let global = enum_global[enum_id.0 as usize] as i64;
                let global_v = fb.ins().iconst(types::I64, global);
                let disc_v = fb.ins().iconst(types::I64, v.discriminant);
                let f = module.declare_func_in_func(panic_aux.enum_unit_get, fb.func);
                let call = fb.ins().call(f, &[global_v, disc_v]);
                let ptr = fb.inst_results(call)[0];
                vmap.insert(*dst, ptr);
                return Ok(());
            }
            // Payload variant — register with the rc-tracked enum
            // registry via __enum_alloc so the cell can be freed on
            // rc=0. Layout still `[tag | payload...]`; the registry
            // sits beside the cell holding (rc, total_bytes).
            let global = enum_global[enum_id.0 as usize] as i64;
            let global_v = fb.ins().iconst(types::I64, global);
            let n_v = fb.ins().iconst(types::I64, n_payload);
            let disc_v = fb.ins().iconst(types::I64, v.discriminant);
            let alloc_fn = module.declare_func_in_func(panic_aux.enum_alloc, fb.func);
            let call = fb.ins().call(alloc_fn, &[global_v, n_v, disc_v]);
            let ptr = fb.inst_results(call)[0];
            for (i, p) in payload.iter().enumerate() {
                let v_ext = extend_to_i64(fb, vmap[p]);
                fb.ins().store(
                    MemFlags::trusted(),
                    v_ext,
                    ptr,
                    8 + (i as i32) * 8,
                );
            }
            vmap.insert(*dst, ptr);
        }
        Inst::EnumTag { dst, value } => {
            let p = vmap[value];
            let v = fb.ins().load(types::I64, MemFlags::trusted(), p, 0);
            vmap.insert(*dst, v);
        }
        Inst::EnumDiscStr { dst, enum_id, value } => {
            // `enum-as-string` cast for `: string`-repr enums.
            // Load the box's tag (variant index), then call
            // `__enum_disc_str(global, tag)` to get a fresh
            // `StringRc *` with the variant's declared
            // discriminant string.
            let p = vmap[value];
            let tag = fb.ins().load(types::I64, MemFlags::trusted(), p, 0);
            let global = enum_global[enum_id.0 as usize] as i64;
            let global_v = fb.ins().iconst(types::I64, global);
            let f = module.declare_func_in_func(panic_aux.enum_disc_str, fb.func);
            let call = fb.ins().call(f, &[global_v, tag]);
            let v = fb.inst_results(call)[0];
            vmap.insert(*dst, v);
        }
        Inst::EnumPayload { dst, value, variant: _, idx } => {
            let p = vmap[value];
            let off = 8 + (*idx as i32) * 8;
            let raw = fb.ins().load(types::I64, MemFlags::trusted(), p, off);
            let dst_ty = func.ty_of(*dst).clone();
            let v = reduce_from_i64(fb, &dst_ty, raw);
            // Heap-typed payload extraction transfers ownership: the
            // extract sees the cell's stored +1 and gives the caller
            // its own +1. Pairs with `host_release_enum`'s cascade
            // on the cell's drop — without the retain, the
            // arm-scope release of the extracted binding would
            // double-decrement and either dangle (cell still holds
            // the ptr) or crash on subsequent access.
            let kind = kind_tag_of(&dst_ty, &prog.classes);
            if kind != KIND_NONE {
                let r = match kind {
                    KIND_OBJECT => panic_aux.retain_obj,
                    KIND_ARRAY => panic_aux.retain_array,
                    KIND_OPTIONAL => panic_aux.retain_optional,
                    KIND_TUPLE => panic_aux.retain_tuple,
                    KIND_MAP => panic_aux.retain_map,
                    KIND_CLOSURE => panic_aux.retain_closure,
                    KIND_STR => panic_aux.retain_string,
                    KIND_ENUM => panic_aux.retain_enum,
                    KIND_PROMISE => panic_aux.retain_promise,
                    _ => unreachable!(),
                };
                let f = module.declare_func_in_func(r, fb.func);
                fb.ins().call(f, &[v]);
            }
            vmap.insert(*dst, v);
        }
        Inst::NewTuple { dst, items } => {
            // Heterogeneous fixed-arity product. Hidden 16-byte
            // header lives BEFORE the user-facing pointer:
            //   base + 0  = rc
            //   base + 8  = packed:
            //                 bits  0-15 = arity (max 65535)
            //                 bits 16-63 = 4-bit KIND_* tag per
            //                              element (up to 12 elements;
            //                              elements 12+ leak any
            //                              heap content but the cell
            //                              itself is still freed).
            //   base + 16 = element 0 ← user_ptr
            // TupleExtract reads from offset 0 of user_ptr, unchanged.
            let n = items.len() as i64;
            let bytes = fb.ins().iconst(types::I64, 16 + n.max(1) * 8);
            let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
            let call = fb.ins().call(alloc_ref, &[bytes]);
            let base = fb.inst_results(call)[0];
            let off16 = fb.ins().iconst(types::I64, 16);
            let ptr = fb.ins().iadd(base, off16);
            // rc = 1
            let one = fb.ins().iconst(types::I64, 1);
            fb.ins().store(MemFlags::trusted(), one, base, 0);
            // packed (kinds | arity)
            let dst_ty = func.ty_of(*dst).clone();
            let mut packed: i64 = n & 0xFFFF;
            if let MirTy::Tuple(elems) = &dst_ty {
                for (i, ety) in elems.iter().enumerate() {
                    if i >= 12 {
                        break;
                    }
                    let kind = kind_tag_of(ety, &prog.classes) & 0xF;
                    packed |= kind << (16 + (i as i64) * 4);
                }
            }
            let mask_v = fb.ins().iconst(types::I64, packed);
            fb.ins().store(MemFlags::trusted(), mask_v, base, 8);
            for (i, it) in items.iter().enumerate() {
                let v_ext = extend_to_i64(fb, vmap[it]);
                fb.ins().store(MemFlags::trusted(), v_ext, ptr, (i as i32) * 8);
            }
            vmap.insert(*dst, ptr);
        }
        Inst::TupleExtract { dst, tup, idx } => {
            let p = vmap[tup];
            let off = (*idx as i32) * 8;
            let raw = fb.ins().load(types::I64, MemFlags::trusted(), p, off);
            let dst_ty = func.ty_of(*dst).clone();
            let v = reduce_from_i64(fb, &dst_ty, raw);
            vmap.insert(*dst, v);
        }
        Inst::NewOptional { dst, value } => {
            // `some(v)` → allocate a 3-cell heap [value | rc | kind_tag]
            // and return its address. `value` is at offset 0 so existing
            // unwrap / iflet paths keep reading from offset 0.
            let bytes = fb.ins().iconst(types::I64, 24);
            let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
            let call = fb.ins().call(alloc_ref, &[bytes]);
            let ptr = fb.inst_results(call)[0];
            let v_ext = extend_to_i64(fb, vmap[value]);
            fb.ins().store(MemFlags::trusted(), v_ext, ptr, 0);
            let one = fb.ins().iconst(types::I64, 1);
            fb.ins().store(MemFlags::trusted(), one, ptr, 8);
            // Tag from the dst's static type — kind_tag mirrors the
            // Array convention: KIND_* discriminant of the inner
            // type so host_release_optional can dispatch the right
            // release fn at cascade time.
            let dst_ty = func.ty_of(*dst).clone();
            let tag = if let MirTy::Optional(inner) = &dst_ty {
                kind_tag_of(inner, &prog.classes)
            } else {
                KIND_NONE
            };
            let tag_v = fb.ins().iconst(types::I64, tag);
            fb.ins().store(MemFlags::trusted(), tag_v, ptr, 16);
            vmap.insert(*dst, ptr);
        }
        Inst::OptionalIsSome { dst, opt } => {
            let p = vmap[opt];
            let zero = fb.ins().iconst(types::I64, 0);
            let v = fb.ins().icmp(IntCC::NotEqual, p, zero);
            vmap.insert(*dst, v);
        }
        Inst::OptionalUnwrap { dst, opt } => {
            let p = vmap[opt];
            let zero = fb.ins().iconst(types::I64, 0);
            let is_none = fb.ins().icmp(IntCC::Equal, p, zero);
            emit_panic_if(fb, module, panic_aux.fn_id, panic_aux.msg_unwrap, is_none);
            let raw = fb.ins().load(types::I64, MemFlags::trusted(), p, 0);
            let dst_ty = func.ty_of(*dst).clone();
            let v = reduce_from_i64(fb, &dst_ty, raw);
            vmap.insert(*dst, v);
        }
        Inst::LoadField { dst, obj, field } => {
            objects::lower_load_field(
                fb, dst, obj, field, vmap, func, fn_ids, builtin_ids,
                static_data, string_data, alloc_id, map_ids, promise_ids, str_ids,
                print_ids, panic_aux, print_lits, module, locals, prog,
                env_value, class_global, enum_global,
                class_struct_global, stack_local,
            )?;
        }
        Inst::StoreField { obj, field, value } => {
            objects::lower_store_field(
                fb, obj, field, value, vmap, func, fn_ids, builtin_ids,
                static_data, string_data, alloc_id, map_ids, promise_ids, str_ids,
                print_ids, panic_aux, print_lits, module, locals, prog,
                env_value, class_global, enum_global,
                class_struct_global, stack_local,
            )?;
        }
        Inst::LoadStatic { dst, slot } => {
            let did = *static_data.get(slot).ok_or_else(|| {
                CompileError::Other(format!("missing static data slot #{}", slot.0))
            })?;
            let gv = module.declare_data_in_func(did, fb.func);
            let addr = fb
                .ins()
                .symbol_value(types::I64, gv);
            // Load type matches the slot's declared MirTy.
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
        _ => {
            return Err(CompileError::Unsupported(
                "MIR inst kind not yet wired in mir-codegen",
            ));
        }
    }
    Ok(())
}
