//! `Inst::Call` lowering — by far the biggest variant in
//! `lower_inst`. Extracted from `lower_inst/mod.rs` so the dispatch
//! match stays scannable. Helpers here are called exactly once from
//! the corresponding arm.

#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature};
use cranelift_frontend::{FunctionBuilder as ClifFnBuilder, Variable};
use cranelift_module::{DataId, Module};

use ilang_ast::Symbol;
use ilang_mir::{
    FuncId, FuncRef, Function as MirFunction, MirTy, Program, StaticSlotId, ValueId,
};

use crate::ty::mir_to_clif;

use super::super::abi::{
    chunk_max_for, elem_byte_stride, elem_clif_type, struct_byval_size_with_max,
    struct_chunks_with_max, struct_hfa, struct_indirect_with_max,
};
use super::super::print_emit::emit_print_value;
use super::super::{
    CompileError, MapIds, PanicAux, PrintIds, PrintLits, PromiseIds, StrIds,
};


pub(super) fn lower_call<M: Module>(
    fb: &mut ClifFnBuilder,
    dst: &Option<ValueId>, callee: &FuncRef, args: &[ValueId],
    vmap: &mut HashMap<ValueId, Value>,
    func: &MirFunction,
    fn_ids: &HashMap<FuncId, cranelift_module::FuncId>,
    builtin_ids: &HashMap<String, (cranelift_module::FuncId, Signature)>,
    _static_data: &HashMap<StaticSlotId, DataId>,
    _string_data: &HashMap<Symbol, DataId>,
    alloc_id: cranelift_module::FuncId,
    map_ids: MapIds,
    promise_ids: PromiseIds,
    str_ids: StrIds,
    print_ids: PrintIds,
    panic_aux: PanicAux,
    print_lits: PrintLits,
    module: &mut M,
    _locals: &[Variable],
    prog: &Program,
    _env_value: Value,
    _class_global: &[u32],
    enum_global: &[u32],
    class_struct_global: &[i64],
    _stack_local: &std::collections::HashSet<ValueId>,
) -> Result<(), CompileError> {
    // `console.log(...)` — special-cased variadic. Each
    // argument prints with a per-type host helper, separated
    // by spaces and terminated by a newline.
    if let FuncRef::Builtin(sym) = callee {
        if sym.as_str() == "console_log" {
            // Skip Unit-typed args entirely. The CLI's
            // `wrap_trailing_print` may pass them when a
            // program's trailing expression is a void method
            // call (e.g. `test.expect(...)`); in that case
            // nothing should be printed and the trailing
            // newline is suppressed too so stdout stays clean.
            let mut printed = 0usize;
            for a in args.iter() {
                let aty = func.ty_of(*a).clone();
                if matches!(aty, MirTy::Unit) {
                    continue;
                }
                if printed > 0 {
                    let r = module.declare_func_in_func(print_ids.space, fb.func);
                    fb.ins().call(r, &[]);
                }
                let av = vmap[a];
                emit_print_value(fb, module, print_ids, print_lits, &aty, av, enum_global, class_struct_global);
                printed += 1;
            }
            if printed > 0 {
                let r = module.declare_func_in_func(print_ids.newline, fb.func);
                fb.ins().call(r, &[]);
            }
            if let Some(d) = dst {
                // console.log returns Unit — produce a sentinel
                // for any (unlikely) consumer.
                let sentinel = fb.ins().iconst(types::I8, 0);
                vmap.insert(*d, sentinel);
            }
            return Ok(());
        }
    }
    let mut arg_vs: Vec<Value> = Vec::with_capacity(args.len());
    // Resolve callee FuncId early so the by-value chunk schema
    // matches what `clif_signature_for` declared on the callee
    // side (C ABI vs ilang ABI cap).
    let (callee_cid, is_callee_extern, is_callee_builtin, callee_chunk_max) = match callee {
        FuncRef::Local(id) => {
            let target_func = &prog.functions[id.0 as usize];
            let is_extern_callee =
                matches!(target_func.kind, ilang_mir::FunctionKind::Extern { .. });
            let cid = *fn_ids.get(id).ok_or_else(|| {
                CompileError::Other(format!("missing fn id #{}", id.0))
            })?;
            (Some(cid), is_extern_callee, false, chunk_max_for(target_func))
        }
        // Builtins don't take CRepr struct args by the chunk path —
        // any threshold works, pick the C one defensively.
        _ => (None, false, false, super::super::abi::C_BYVAL_CHUNK_MAX),
    };
    // sret: pre-alloc the destination struct and pass its pointer
    // as the hidden first arg. Triggered when the callee returns a
    // CRepr struct that doesn't fit in its ABI's chunk budget.
    let sret_dst = if let Some(d) = dst {
        let dst_ty = func.ty_of(*d).clone();
        if let Some(c_size) =
            struct_indirect_with_max(&dst_ty, prog, callee_chunk_max)
        {
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
    let _ = is_callee_extern;
    // Builtins like array_map / array_filter / array_for_each /
    // array_slice / array_index_of / array_includes consume a
    // dynamic-array header (6×i64 [len|cap|data|rc|kind|stride]).
    // Fixed-length arrays carry no header — they're just inline
    // element data — so wrap them on-the-fly via __fixed_to_dyn
    // so the receiver sees a uniform header shape.
    let wrap_fixed_first_arg: Option<i64> = if let FuncRef::Builtin(sym) = callee {
        let kind_tag = match sym.as_str() {
            "array_map"
            | "array_filter"
            | "array_for_each"
            | "array_slice"
            | "array_index_of"
            | "array_includes" => Some(0i64),
            _ => None,
        };
        kind_tag
    } else {
        None
    };
    for (arg_ix, a) in args.iter().enumerate() {
        let mut av = *vmap.get(a).unwrap_or_else(|| {
            panic!(
                "missing vmap entry for arg {:?} in call to {:?}",
                a, callee
            )
        });
        if let (Some(kind_tag_for_obj), 0) = (wrap_fixed_first_arg, arg_ix) {
            if let MirTy::Array { elem, len: Some(n) } = func.ty_of(*a) {
                let stride = elem_byte_stride(elem);
                let kind_tag = if matches!(**elem, MirTy::Object(_)) {
                    1
                } else {
                    kind_tag_for_obj
                };
                let len_v = fb.ins().iconst(types::I64, *n as i64);
                let stride_v = fb.ins().iconst(types::I64, stride);
                let kind_v = fb.ins().iconst(types::I64, kind_tag);
                let f = module.declare_func_in_func(str_ids.fixed_to_dyn, fb.func);
                let call = fb.ins().call(f, &[av, len_v, stride_v, kind_v]);
                av = fb.inst_results(call)[0];
            }
        }
        // CRepr / CPacked struct arg → chunk / HFA explode regardless
        // of callee kind. Builtins (str_*, array_*, etc.) don't take
        // user-defined CRepr types so they fall through to the plain
        // arg_vs.push below.
        if !is_callee_builtin {
            let aty = func.ty_of(*a);
            if let Some((elem_ct, count)) = struct_hfa(aty, prog) {
                let elem_size: i32 = if elem_ct == types::F32 { 4 } else { 8 };
                for c in 0..count {
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
            if let Some(chunks) = struct_chunks_with_max(aty, prog, callee_chunk_max) {
                for c in 0..chunks {
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
            // CRepr above the callee's chunk cap (non-HFA, non-
            // chunkable): emit the caller-side memcpy manually.
            // Cranelift's `StructArgument(size)` purpose would do
            // the same thing but it isn't implemented on AArch64.
            // Allocate a scratch StackSlot of c_size, byte-copy the
            // source struct into it, and pass that slot's pointer —
            // the callee can mutate fields freely without reaching
            // back into the caller's value.
            if let Some(size) = struct_byval_size_with_max(aty, prog, callee_chunk_max) {
                let slot = fb.create_sized_stack_slot(
                    cranelift_codegen::ir::StackSlotData::new(
                        cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                        size as u32,
                        3,
                    ),
                );
                let copy_ptr = fb.ins().stack_addr(types::I64, slot, 0);
                let mut off: i32 = 0;
                while (off as i64) < size {
                    let cell = fb.ins().load(types::I64, MemFlags::trusted(), av, off);
                    fb.ins().store(MemFlags::trusted(), cell, copy_ptr, off);
                    off += 8;
                }
                arg_vs.push(copy_ptr);
                continue;
            }
        }
        arg_vs.push(av);
    }
    let (cid, is_builtin) = match callee {
        FuncRef::Local(_) => (callee_cid.unwrap(), is_callee_extern),
        FuncRef::Builtin(sym) => {
            // FFI marshalling helpers are declared by name —
            // route them via `module.declarations` lookup so we
            // don't need a separate id table.
            if matches!(
                sym.as_str(),
                "cstrFromString"
                    | "stringFromCstr"
                    | "cstrArrayToStrings"
                    | "__array_data_ptr"
                    | "__enum_box"
                    | "__c_array_to_array"
                    | "__repl_load_slot"
                    | "__repl_store_slot"
                    | "__read_i8" | "__read_i16" | "__read_i32" | "__read_i64"
                    | "__read_u8" | "__read_u16" | "__read_u32" | "__read_u64"
                    | "__read_f32" | "__read_f64"
                    | "__write_i8" | "__write_i16" | "__write_i32" | "__write_i64"
                    | "__write_u8" | "__write_u16" | "__write_u32" | "__write_u64"
                    | "__write_f32" | "__write_f64"
                    | "freeCstr"
                    | "errnoCheck"
                    | "errnoCheckI64"
                    | "os.errno"
                    | "os.setErrno"
            ) {
                let cid = module
                    .declarations()
                    .get_name(sym.as_str())
                    .and_then(|n| match n {
                        cranelift_module::FuncOrDataId::Func(id) => Some(id),
                        _ => None,
                    })
                    .ok_or_else(|| {
                        CompileError::Other(format!(
                            "ffi helper `{sym}` not declared"
                        ))
                    })?;
                (cid, true)
            } else {
            // Translate well-known MIR builtin names to the
            // host-registered Cranelift FuncIds.
            let host_id = match sym.as_str() {
                "str_length" => Some(str_ids.length),
                "str_concat" => Some(str_ids.concat),
                "str_eq" => Some(str_ids.eq),
                "int_to_string" => Some(str_ids.int_to_string),
                "bool_to_string" => Some(str_ids.bool_to_string),
                "str_to_upper" => Some(str_ids.to_upper),
                "str_to_lower" => Some(str_ids.to_lower),
                "str_trim" => Some(str_ids.trim),
                "str_includes" => Some(str_ids.includes),
                "str_starts_with" => Some(str_ids.starts_with),
                "str_ends_with" => Some(str_ids.ends_with),
                "str_char_at" => Some(str_ids.char_at),
                "str_slice" => Some(str_ids.slice),
                "str_replace" => Some(str_ids.replace),
                "array_index_of" => Some(str_ids.array_index_of),
                "array_includes" => Some(str_ids.array_includes),
                "array_push" => Some(str_ids.array_push),
                "array_pop" => Some(str_ids.array_pop),
                "array_map" => Some(str_ids.array_map),
                "array_filter" => Some(str_ids.array_filter),
                "array_for_each" => Some(str_ids.array_for_each),
                "array_slice" => Some(str_ids.array_slice),
                "str_split" => Some(str_ids.str_split),
                "map_get" => Some(map_ids.get),
                "map_get_optional" => Some(map_ids.get_optional),
                "map_set" => Some(map_ids.set),
                "map_size" => Some(map_ids.size),
                "map_has" => Some(map_ids.has),
                "map_delete" => Some(map_ids.delete),
                "map_keys" => Some(map_ids.keys),
                "map_values" => Some(map_ids.values),
                "promise_resolve" => Some(promise_ids.resolve),
                "promise_reject" => Some(promise_ids.reject),
                "promise_then" => Some(promise_ids.then),
                "promise_catch" => Some(promise_ids.catch),
                "promise_with_executor" => Some(promise_ids.with_executor),
                "promise_drain" => Some(promise_ids.drain),
                "promise_all" => Some(promise_ids.all),
                "promise_race" => Some(promise_ids.race),
                "promise_pending" => Some(promise_ids.pending),
                "promise_settle_resolve" => Some(promise_ids.settle_resolve),
                "promise_settle_reject" => Some(promise_ids.settle_reject),
                "class_name" => Some(panic_aux.class_name),
                _ => None,
            };
            let cid = match host_id {
                Some(id) => id,
                None => {
                    builtin_ids
                        .get(sym.as_str())
                        .ok_or_else(|| {
                            CompileError::Other(format!(
                                "unregistered builtin `{sym}`"
                            ))
                        })?
                        .0
                }
            };
            (cid, true)
            }
        }
        FuncRef::Extern { .. } => {
            return Err(CompileError::Unsupported("extern call"));
        }
    };
    // Local fns carry the unified env-trailing param; builtins
    // don't.
    if !is_builtin {
        let zero = fb.ins().iconst(types::I64, 0);
        arg_vs.push(zero);
    }
    // For builtins like the map / array / str runtime, the
    // declared sig is uniformly i64. Auto-extend any narrower
    // arg so the verifier doesn't complain (bool/i32/f64
    // bitcast to i64). Signed MIR ints sign-extend; unsigned
    // / bool / raw bit patterns zero-extend. Without the
    // signed branch, e.g. `(-1: i32).toString()` would pass
    // `4294967295` to `__int_to_string` and display the
    // unsigned bit pattern instead of `-1` (mirrored across
    // i8 / i16 / i32 — see int_to_string_signed.il).
    if is_builtin {
        let sig_params = module.declarations()
            .get_function_decl(cid)
            .signature
            .params
            .clone();
        for (i, av) in arg_vs.iter_mut().enumerate() {
            let want = match sig_params.get(i) {
                Some(p) => p.value_type,
                None => continue,
            };
            let got = fb.func.dfg.value_type(*av);
            if got == want {
                continue;
            }
            if want == types::I64 {
                if got == types::F64 {
                    *av = fb.ins().bitcast(types::I64, MemFlags::new(), *av);
                } else if got == types::F32 {
                    let r32 = fb.ins().bitcast(types::I32, MemFlags::new(), *av);
                    *av = fb.ins().uextend(types::I64, r32);
                } else if got.is_int() && got.bits() < 64 {
                    // arg_vs is index-aligned with `args` for
                    // builtin calls (no sret prefix, no trailing
                    // env). Look up the MIR type to choose the
                    // sign-correct widening.
                    let signed = args
                        .get(i)
                        .map(|a| func.ty_of(*a).is_signed_int())
                        .unwrap_or(false);
                    *av = if signed {
                        fb.ins().sextend(types::I64, *av)
                    } else {
                        fb.ins().uextend(types::I64, *av)
                    };
                }
            }
        }
    }
    let local_ref = module.declare_func_in_func(cid, fb.func);
    // C-variadic extern: build a per-call signature with the
    // actual arg types and dispatch via call_indirect (the
    // declared signature only covers the fixed prefix). On
    // Apple AArch64 the variadic ABI pads the integer / FP
    // register files so the variadic tail spills to the stack
    // — fill the spare slots with zero placeholders.
    let variadic_dispatch = if is_callee_extern {
        if let FuncRef::Local(fid) = callee {
            let target = &prog.functions[fid.0 as usize];
            if target.is_variadic && arg_vs.len() > target.params.len() {
                Some(target.params.len())
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };
    let inst_ref = if let Some(n_fixed) = variadic_dispatch {
        let mut cl_sig = module.make_signature();
        let needs_apple_pad =
            cfg!(target_os = "macos") && cfg!(target_arch = "aarch64");
        let fixed: Vec<Value> = arg_vs[..n_fixed].to_vec();
        let varargs: Vec<Value> = arg_vs[n_fixed..].to_vec();
        for v in &fixed {
            cl_sig.params.push(AbiParam::new(fb.func.dfg.value_type(*v)));
        }
        let mut padded: Vec<Value> = fixed.clone();
        if needs_apple_pad && !varargs.is_empty() {
            let n_int_fixed = fixed
                .iter()
                .filter(|v| fb.func.dfg.value_type(**v).is_int())
                .count();
            let n_fp_fixed = fixed
                .iter()
                .filter(|v| fb.func.dfg.value_type(**v).is_float())
                .count();
            let n_int_pad = 8usize.saturating_sub(n_int_fixed);
            let n_fp_pad = 8usize.saturating_sub(n_fp_fixed);
            for _ in 0..n_int_pad {
                cl_sig.params.push(AbiParam::new(types::I64));
            }
            for _ in 0..n_fp_pad {
                cl_sig.params.push(AbiParam::new(types::F64));
            }
            let zero_i = fb.ins().iconst(types::I64, 0);
            let zero_f = fb.ins().f64const(0.0);
            for _ in 0..n_int_pad {
                padded.push(zero_i);
            }
            for _ in 0..n_fp_pad {
                padded.push(zero_f);
            }
        }
        for v in &varargs {
            cl_sig.params.push(AbiParam::new(fb.func.dfg.value_type(*v)));
            padded.push(*v);
        }
        let target_func = match callee {
            FuncRef::Local(fid) => &prog.functions[fid.0 as usize],
            _ => unreachable!(),
        };
        if !matches!(target_func.ret, MirTy::Unit) {
            if let Some(rt) = elem_clif_type(&target_func.ret) {
                cl_sig.returns.push(AbiParam::new(rt));
            } else {
                cl_sig.returns.push(AbiParam::new(types::I64));
            }
        }
        let sig_ref = fb.import_signature(cl_sig);
        let func_addr = fb.ins().func_addr(types::I64, local_ref);
        fb.ins().call_indirect(sig_ref, func_addr, &padded)
    } else {
        fb.ins().call(local_ref, &arg_vs)
    };
    // sret: the call has no clif return; the pre-alloc'd
    // pointer is what the user sees.
    if let Some((d, ptr)) = sret_dst {
        vmap.insert(d, ptr);
        return Ok(());
    }
    if let Some(d) = dst {
        let dst_ty = func.ty_of(*d).clone();
        // Returned CRepr struct: result arrives as HFA float regs or
        // i64 chunks. Reassemble into a heap buffer and bind that as
        // the SSA value's pointer. Escape analysis may later promote
        // this allocation to a StackSlot at the NewObject site, but
        // call-result buffers always go through alloc.
        if !is_callee_builtin {
            if let Some((elem_ct, count)) = struct_hfa(&dst_ty, prog) {
                let layout = if let MirTy::Object(cid) = &dst_ty {
                    &prog.classes[cid.0 as usize]
                } else {
                    unreachable!()
                };
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
                return Ok(());
            }
            if let Some(chunks) = struct_chunks_with_max(&dst_ty, prog, callee_chunk_max) {
                let layout = if let MirTy::Object(cid) = &dst_ty {
                    &prog.classes[cid.0 as usize]
                } else {
                    unreachable!()
                };
                let size_v = fb.ins().iconst(types::I64, layout.c_size.max(1));
                let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
                let alloc_call = fb.ins().call(alloc_ref, &[size_v]);
                let ptr = fb.inst_results(alloc_call)[0];
                let results: Vec<Value> = fb.inst_results(inst_ref).to_vec();
                for (i, &chunk) in results.iter().take(chunks).enumerate() {
                    fb.ins().store(
                        MemFlags::trusted(),
                        chunk,
                        ptr,
                        (i as i32) * 8,
                    );
                }
                vmap.insert(*d, ptr);
                return Ok(());
            }
        }
        let _ = is_callee_extern;
        let results = fb.inst_results(inst_ref);
        if let Some(&v) = results.first() {
            let v_clif = fb.func.dfg.value_type(v);
            let want = mir_to_clif(&dst_ty);
            let v_adj = match (want, v_clif) {
                (Some(target), got) if target.bits() < got.bits() => {
                    fb.ins().ireduce(target, v)
                }
                _ => v,
            };
            vmap.insert(*d, v_adj);
        }
    }
    Ok(())
}
