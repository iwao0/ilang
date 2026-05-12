//! Per-instruction MIR → cranelift lowering. The bulk of `compile/`
//! lives here: each `Inst` variant emits the cranelift sequence that
//! realises it, with the surrounding `BodyCx`-style state threaded in
//! as parameters from `lower_function`.

use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature};
use cranelift_frontend::{FunctionBuilder as ClifFnBuilder, Variable};
use cranelift_module::{DataId, Module};

use ilang_ast::Symbol;
use ilang_mir::{
    BinOp, FuncId, FuncRef, Function as MirFunction, Inst, MirConst, MirTy, Program,
    StaticSlotId, UnOp, ValueId,
};

use crate::ty::mir_to_clif;

use super::abi::{
    celem_clif_type_with_enum, elem_byte_stride, elem_clif_type, extend_to_i64, ireduce_or_pass,
    reduce_from_i64, struct_chunks, struct_hfa, struct_indirect,
};
use super::binop_cast::{lower_binop, lower_cast};
use super::lower_term_const::lower_const;
use super::print_emit::{emit_panic_if, emit_print_value};
use super::print_kind::{
    kind_tag_of, print_kind_id, KIND_ARRAY, KIND_CLOSURE, KIND_ENUM, KIND_MAP, KIND_NONE,
    KIND_OBJECT, KIND_OPTIONAL, KIND_STR, KIND_TUPLE,
};
use super::{
    emit_is_subclass, CompileError, MapIds, PanicAux, PrintIds, PrintLits, StrIds,
    OBJECT_HEADER_BYTES,
};

pub(super) fn lower_inst<M: Module>(
    fb: &mut ClifFnBuilder,
    inst: &Inst,
    vmap: &mut HashMap<ValueId, Value>,
    func: &MirFunction,
    fn_ids: &HashMap<FuncId, cranelift_module::FuncId>,
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
    locals: &[Variable],
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
            // Resolve callee FuncId early so we can know whether it's
            // extern (and split CRepr struct args into chunks).
            let (callee_cid, is_callee_extern, is_callee_builtin) = match callee {
                FuncRef::Local(id) => {
                    let target_func = &prog.functions[id.0 as usize];
                    let is_extern_callee =
                        matches!(target_func.kind, ilang_mir::FunctionKind::Extern { .. });
                    let cid = *fn_ids.get(id).ok_or_else(|| {
                        CompileError::Other(format!("missing fn id #{}", id.0))
                    })?;
                    (Some(cid), is_extern_callee, false)
                }
                _ => (None, false, false),
            };
            // sret: pre-alloc the destination struct and pass its
            // pointer as the hidden first arg.
            let sret_dst = if is_callee_extern {
                if let Some(d) = dst {
                    let dst_ty = func.ty_of(*d).clone();
                    if let Some(c_size) = struct_indirect(&dst_ty, prog) {
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
                }
            } else {
                None
            };
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
                if is_callee_extern {
                    let aty = func.ty_of(*a);
                    if let Some((elem_ct, count)) = struct_hfa(aty, prog) {
                        // Read `count` floats from the struct body
                        // (offset = i × elem_byte_size).
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
                    if let Some(chunks) = struct_chunks(aty, prog) {
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
                }
                arg_vs.push(av);
            }
            let _ = is_callee_builtin;
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
                if is_callee_extern {
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
                    if let Some(chunks) = struct_chunks(&dst_ty, prog) {
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
        }
        Inst::VirtCall { dst, recv, slot, args } => {
            // Load class_id from object header, dispatch via the
            // host runtime helper, then call_indirect.
            let recv_v = vmap[recv];
            let cid = fb.ins().load(types::I64, MemFlags::trusted(), recv_v, 0);
            let slot_v = fb.ins().iconst(types::I64, slot.0 as i64);
            let dispatch_ref = module.declare_func_in_func(str_ids.virt_dispatch, fb.func);
            let lookup = fb.ins().call(dispatch_ref, &[cid, slot_v]);
            let fn_ptr = fb.inst_results(lookup)[0];
            // Build a clif sig matching the method ABI: this + args + env.
            let mut clif_sig = module.make_signature();
            clif_sig.params.push(AbiParam::new(types::I64));
            // Other params: re-derive from the receiver's class
            // method's MIR sig. For simplicity treat each arg's clif
            // type as its current value's type at the call site.
            for a in args.iter() {
                let ty = fb.func.dfg.value_type(vmap[a]);
                clif_sig.params.push(AbiParam::new(ty));
            }
            clif_sig.params.push(AbiParam::new(types::I64)); // env
            let dst_ty_mir = dst.map(|d| func.ty_of(d).clone());
            if let Some(t) = &dst_ty_mir {
                if !matches!(t, MirTy::Unit) {
                    if let Some(ct) = mir_to_clif(t) {
                        clif_sig.returns.push(AbiParam::new(ct));
                    }
                }
            }
            let sig_ref = fb.import_signature(clif_sig);
            let mut arg_vs: Vec<Value> = vec![recv_v];
            for a in args.iter() {
                arg_vs.push(vmap[a]);
            }
            let zero = fb.ins().iconst(types::I64, 0);
            arg_vs.push(zero);
            let inst_ref = fb.ins().call_indirect(sig_ref, fn_ptr, &arg_vs);
            if let Some(d) = dst {
                let results = fb.inst_results(inst_ref);
                if let Some(&v) = results.first() {
                    vmap.insert(*d, v);
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
            let var = locals[local.0 as usize];
            let v = vmap[value];
            if std::env::var("ILANG_DEBUG_DEFLOCAL").is_ok() {
                let want = func.local_tys[local.0 as usize].clone();
                let got = fb.func.dfg.value_type(v);
                eprintln!(
                    "[deflocal] fn={} local#{} declared={want} clif_val_ty={got}",
                    func.name.as_str(), local.0
                );
            }
            fb.def_var(var, v);
        }
        Inst::UseLocal { dst, local } => {
            let var = locals[local.0 as usize];
            let v = fb.use_var(var);
            vmap.insert(*dst, v);
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
                // CRepr struct alloc. With a flexible array tail
                // (`new packet(n)`) the user passes the FAM length
                // as the first arg; total size = c_size +
                // n*flex_elem_size.
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
                let ptr = fb.inst_results(alloc_call)[0];
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
            // Inline fixed-length output (when the dst MirTy carries
            // `len: Some(n)`): allocate `n*stride` bytes with no
            // header, store elements directly at `data + i*stride`.
            // This keeps the layout consistent with array fields of
            // `@extern(C)` structs that LoadField returns as inline
            // addresses.
            let dst_ty = func.ty_of(*dst).clone();
            if let MirTy::Array { len: Some(_), .. } = &dst_ty {
                let stride_bytes = elem_byte_stride(elem);
                let n = items.len() as i64;
                let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
                let bytes = fb.ins().iconst(types::I64, n.max(1) * stride_bytes);
                let call = fb.ins().call(alloc_ref, &[bytes]);
                let ptr = fb.inst_results(call)[0];
                let elem_clif_opt = elem_clif_type(elem);
                for (i, it) in items.iter().enumerate() {
                    let raw = vmap[it];
                    let off = (i as i32) * (stride_bytes as i32);
                    if let Some(elem_ct) = elem_clif_opt {
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
            // the right slots.
            let stride_bytes = elem_byte_stride(elem);
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
            let tag = kind_tag_of(elem);
            let tag_v = fb.ins().iconst(types::I64, tag);
            fb.ins().store(MemFlags::trusted(), tag_v, ptr, 32);
            let stride_v = fb.ins().iconst(types::I64, stride_bytes);
            fb.ins().store(MemFlags::trusted(), stride_v, ptr, 40);
            let elem_clif_opt = elem_clif_type(elem);
            for (i, it) in items.iter().enumerate() {
                let raw = vmap[it];
                let off = (i as i32) * (stride_bytes as i32);
                if let Some(elem_ct) = elem_clif_opt {
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
            let tag = kind_tag_of(elem);
            let tag_v = fb.ins().iconst(types::I64, tag);
            fb.ins().store(MemFlags::trusted(), tag_v, ptr, 32);
            let stride_v = fb.ins().iconst(types::I64, stride_bytes);
            fb.ins().store(MemFlags::trusted(), stride_v, ptr, 40);
            vmap.insert(*dst, ptr);
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
            let dst_ty_mir = func.ty_of(*dst);
            let v = match elem_clif_type(dst_ty_mir) {
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
            let val_kind = kind_tag_of(val);
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
            let kind = kind_tag_of(&dst_ty);
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
                    let kind = kind_tag_of(ety) & 0xF;
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
                kind_tag_of(inner)
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
            let obj_v = vmap[obj];
            let dst_ty_mir = func.ty_of(*dst).clone();
            let obj_ty_mir = func.ty_of(*obj).clone();
            let (crepr, bit_info) = if let MirTy::Object(cid) = &obj_ty_mir {
                let layout = &prog.classes[cid.0 as usize];
                if matches!(
                    layout.repr,
                    ilang_mir::ClassRepr::CRepr
                        | ilang_mir::ClassRepr::CPacked
                        | ilang_mir::ClassRepr::CUnion
                ) {
                    let off = layout.c_field_offsets.get(field.0 as usize).copied().unwrap_or(0);
                    let bf = layout
                        .fields
                        .get(field.0 as usize)
                        .and_then(|f| f.bit_field);
                    (Some(off), bf)
                } else {
                    (None, None)
                }
            } else {
                (None, None)
            };
            // Bitfield read: load the storage unit, shift right by
            // bit_offset, mask off the high bits beyond `width`.
            if let (Some(c_off), Some(bf)) = (crepr, bit_info) {
                let storage_ct = match elem_clif_type(&dst_ty_mir) {
                    Some(t) if t.bits() <= 32 => t,
                    _ => types::I32,
                };
                let raw = fb.ins().load(
                    storage_ct,
                    MemFlags::trusted(),
                    obj_v,
                    c_off as i32,
                );
                let shifted = if bf.offset == 0 {
                    raw
                } else {
                    let shift = fb.ins().iconst(storage_ct, bf.offset as i64);
                    fb.ins().ushr(raw, shift)
                };
                let mask_val: u64 = if bf.width >= 64 {
                    u64::MAX
                } else {
                    (1u64 << bf.width) - 1
                };
                let mask = fb.ins().iconst(storage_ct, mask_val as i64);
                let v = fb.ins().band(shifted, mask);
                vmap.insert(*dst, v);
                return Ok(());
            }
            // FAM (C99 flexible array member) — last field of a CRepr
            // struct typed `T[]` (no len). The field has no slot of
            // its own; its data starts at obj_v + c_off and runs to
            // the end of the over-allocated buffer. We don't know the
            // element count statically (caller maintains it in a
            // sibling field), so wrap the inline area in a synthetic
            // dyn-array header with len=i64::MAX so subsequent
            // ArrayLoad / ArrayStore bounds checks become no-ops, but
            // the data pointer aliases the inline buffer so reads
            // and writes hit the real storage.
            if let Some(c_off) = crepr {
                let is_fam = matches!(&dst_ty_mir, MirTy::Array { len: None, .. })
                    && matches!(
                        &obj_ty_mir,
                        MirTy::Object(_cid)
                    );
                if is_fam {
                    if let MirTy::Object(cid) = &obj_ty_mir {
                        let layout = &prog.classes[cid.0 as usize];
                        let last_ix = layout.fields.len().saturating_sub(1);
                        if field.0 as usize == last_ix && layout.flex_elem_size > 0 {
                            let elem = if let MirTy::Array { elem, .. } = &dst_ty_mir {
                                (**elem).clone()
                            } else {
                                MirTy::I64
                            };
                            let stride = layout.flex_elem_size;
                            let kind_tag = if matches!(elem, MirTy::Object(_)) {
                                1
                            } else {
                                0
                            };
                            let off_v = fb.ins().iconst(types::I64, c_off);
                            let inline_ptr = fb.ins().iadd(obj_v, off_v);
                            let len_v = fb.ins().iconst(types::I64, i64::MAX);
                            let stride_v = fb.ins().iconst(types::I64, stride);
                            let kind_v = fb.ins().iconst(types::I64, kind_tag);
                            let f = module.declare_func_in_func(str_ids.fixed_to_dyn, fb.func);
                            let call = fb.ins().call(f, &[inline_ptr, len_v, stride_v, kind_v]);
                            let v = fb.inst_results(call)[0];
                            vmap.insert(*dst, v);
                            return Ok(());
                        }
                    }
                }
                // Unit-only enum field: read the discriminant at the
                // repr's natural width, then look up the cached unit
                // cell so downstream `EnumTag` / `match` see a
                // proper heap-box pointer. The lookup aborts if the
                // value the C side wrote isn't a declared variant —
                // matches the `repr(C)` panic-on-unknown contract
                // discussed in the language design notes.
                if let MirTy::Enum(eid) = &dst_ty_mir {
                    let layout = &prog.enums[eid.0 as usize];
                    let unit_only = layout
                        .variants
                        .iter()
                        .all(|v| matches!(v.payload, ilang_mir::VariantPayload::Unit));
                    if unit_only {
                        let repr_ct = elem_clif_type(&layout.repr).unwrap_or(types::I64);
                        let raw = fb.ins().load(repr_ct, MemFlags::trusted(), obj_v, c_off as i32);
                        let disc_i64 = if repr_ct == types::I64 {
                            raw
                        } else if layout.repr.is_signed_int() {
                            fb.ins().sextend(types::I64, raw)
                        } else {
                            fb.ins().uextend(types::I64, raw)
                        };
                        let global = enum_global[eid.0 as usize] as i64;
                        let global_v = fb.ins().iconst(types::I64, global);
                        let f = module.declare_func_in_func(
                            panic_aux.enum_unit_get_checked,
                            fb.func,
                        );
                        let call = fb.ins().call(f, &[global_v, disc_i64]);
                        let v = fb.inst_results(call)[0];
                        vmap.insert(*dst, v);
                        return Ok(());
                    }
                }
                // CRepr: load with the field's natural type at the
                // computed byte offset. Nested CRepr struct fields
                // return the inline address.
                let v = match elem_clif_type(&dst_ty_mir) {
                    Some(elem_ct) if elem_ct == types::I8 => {
                        fb.ins().load(types::I8, MemFlags::trusted(), obj_v, c_off as i32)
                    }
                    Some(elem_ct) if elem_ct == types::I16 => {
                        fb.ins().load(types::I16, MemFlags::trusted(), obj_v, c_off as i32)
                    }
                    Some(elem_ct) if elem_ct == types::I32 => {
                        fb.ins().load(types::I32, MemFlags::trusted(), obj_v, c_off as i32)
                    }
                    Some(elem_ct) if elem_ct == types::F32 => {
                        fb.ins().load(types::F32, MemFlags::trusted(), obj_v, c_off as i32)
                    }
                    Some(elem_ct) if elem_ct == types::F64 => {
                        fb.ins().load(types::F64, MemFlags::trusted(), obj_v, c_off as i32)
                    }
                    _ => {
                        // Nested CRepr struct, fixed-size array, or
                        // i64-sized field — produce the inline address
                        // (additive offset) for composites, otherwise
                        // load the i64 cell.
                        let returns_inline = match &dst_ty_mir {
                            MirTy::Object(inner_cid) => matches!(
                                prog.classes[inner_cid.0 as usize].repr,
                                ilang_mir::ClassRepr::CRepr
                                    | ilang_mir::ClassRepr::CPacked
                                    | ilang_mir::ClassRepr::CUnion
                            ),
                            MirTy::Array { len: Some(_), .. } => true,
                            _ => false,
                        };
                        if returns_inline {
                            let off_v = fb.ins().iconst(types::I64, c_off);
                            fb.ins().iadd(obj_v, off_v)
                        } else {
                            fb.ins().load(
                                types::I64,
                                MemFlags::trusted(),
                                obj_v,
                                c_off as i32,
                            )
                        }
                    }
                };
                vmap.insert(*dst, v);
            } else {
                let off = OBJECT_HEADER_BYTES + (field.0 as i32) * 8;
                let raw = fb.ins().load(types::I64, MemFlags::trusted(), obj_v, off);
                let v = reduce_from_i64(fb, &dst_ty_mir, raw);
                vmap.insert(*dst, v);
            }
        }
        Inst::StoreField { obj, field, value } => {
            let obj_v = vmap[obj];
            let obj_ty_mir = func.ty_of(*obj).clone();
            let (crepr, bit_info) = if let MirTy::Object(cid) = &obj_ty_mir {
                let layout = &prog.classes[cid.0 as usize];
                if matches!(
                    layout.repr,
                    ilang_mir::ClassRepr::CRepr
                        | ilang_mir::ClassRepr::CPacked
                        | ilang_mir::ClassRepr::CUnion
                ) {
                    let off = layout.c_field_offsets.get(field.0 as usize).copied().unwrap_or(0);
                    let bf = layout
                        .fields
                        .get(field.0 as usize)
                        .and_then(|f| f.bit_field);
                    (Some(off), bf)
                } else {
                    (None, None)
                }
            } else {
                (None, None)
            };
            // Bitfield write: read-modify-write: load storage, mask
            // off the field's bits, OR in the new value's bits at
            // the right offset, store back.
            if let (Some(c_off), Some(bf)) = (crepr, bit_info) {
                let val_ty_mir = func.ty_of(*value).clone();
                let raw_val = vmap[value];
                let storage_ct = match elem_clif_type(&val_ty_mir) {
                    Some(t) if t.bits() <= 32 => t,
                    _ => types::I32,
                };
                let cur = fb.ins().load(
                    storage_ct,
                    MemFlags::trusted(),
                    obj_v,
                    c_off as i32,
                );
                let mask_val: u64 = if bf.width >= 64 {
                    u64::MAX
                } else {
                    (1u64 << bf.width) - 1
                };
                let inv_mask_val = !(mask_val << bf.offset);
                let inv_mask = fb.ins().iconst(storage_ct, inv_mask_val as i64);
                let cleared = fb.ins().band(cur, inv_mask);
                let v_truncated = ireduce_or_pass(fb, raw_val, storage_ct);
                let mask = fb.ins().iconst(storage_ct, mask_val as i64);
                let v_masked = fb.ins().band(v_truncated, mask);
                let v_shifted = if bf.offset == 0 {
                    v_masked
                } else {
                    let shift = fb.ins().iconst(storage_ct, bf.offset as i64);
                    fb.ins().ishl(v_masked, shift)
                };
                let new_val = fb.ins().bor(cleared, v_shifted);
                fb.ins().store(MemFlags::trusted(), new_val, obj_v, c_off as i32);
                return Ok(());
            }
            if let Some(c_off) = crepr {
                let val_ty_mir = func.ty_of(*value).clone();
                let raw = vmap[value];
                // If the field type is itself a CRepr struct, copy
                // the source struct's bytes into the destination's
                // inline region rather than storing the pointer.
                if let MirTy::Object(inner_cid) = &val_ty_mir {
                    let inner_layout = &prog.classes[inner_cid.0 as usize];
                    if matches!(
                        inner_layout.repr,
                        ilang_mir::ClassRepr::CRepr
                            | ilang_mir::ClassRepr::CPacked
                            | ilang_mir::ClassRepr::CUnion
                    ) {
                        let dst_addr = if c_off == 0 {
                            obj_v
                        } else {
                            let off_v = fb.ins().iconst(types::I64, c_off);
                            fb.ins().iadd(obj_v, off_v)
                        };
                        // Inline byte-wise copy of `c_size` bytes —
                        // avoids depending on the JIT's memcpy libcall
                        // resolution, which can race with how mir-codegen
                        // declares its own symbols.
                        let total = inner_layout.c_size.max(0);
                        let mut copied = 0i64;
                        while copied + 8 <= total {
                            let v = fb.ins().load(
                                types::I64,
                                MemFlags::trusted(),
                                raw,
                                copied as i32,
                            );
                            fb.ins().store(
                                MemFlags::trusted(),
                                v,
                                dst_addr,
                                copied as i32,
                            );
                            copied += 8;
                        }
                        while copied + 4 <= total {
                            let v = fb.ins().load(
                                types::I32,
                                MemFlags::trusted(),
                                raw,
                                copied as i32,
                            );
                            fb.ins().store(
                                MemFlags::trusted(),
                                v,
                                dst_addr,
                                copied as i32,
                            );
                            copied += 4;
                        }
                        while copied + 2 <= total {
                            let v = fb.ins().load(
                                types::I16,
                                MemFlags::trusted(),
                                raw,
                                copied as i32,
                            );
                            fb.ins().store(
                                MemFlags::trusted(),
                                v,
                                dst_addr,
                                copied as i32,
                            );
                            copied += 2;
                        }
                        while copied < total {
                            let v = fb.ins().load(
                                types::I8,
                                MemFlags::trusted(),
                                raw,
                                copied as i32,
                            );
                            fb.ins().store(
                                MemFlags::trusted(),
                                v,
                                dst_addr,
                                copied as i32,
                            );
                            copied += 1;
                        }
                        return Ok(());
                    }
                }
                // Unit-only enum field: the SSA value is a heap-box
                // pointer; the C struct slot wants the underlying
                // discriminant. Load tag from the box (offset 0) and
                // narrow to the field's repr width before storing.
                let raw = if let MirTy::Enum(eid) = &val_ty_mir {
                    let layout = &prog.enums[eid.0 as usize];
                    let unit_only = layout
                        .variants
                        .iter()
                        .all(|v| matches!(v.payload, ilang_mir::VariantPayload::Unit));
                    if unit_only {
                        fb.ins().load(types::I64, MemFlags::trusted(), raw, 0)
                    } else {
                        raw
                    }
                } else {
                    raw
                };
                match celem_clif_type_with_enum(prog, &val_ty_mir) {
                    Some(elem_ct) if elem_ct != types::I64 => {
                        let truncated = ireduce_or_pass(fb, raw, elem_ct);
                        fb.ins().store(MemFlags::trusted(), truncated, obj_v, c_off as i32);
                    }
                    _ => {
                        let v_ext = extend_to_i64(fb, raw);
                        fb.ins().store(MemFlags::trusted(), v_ext, obj_v, c_off as i32);
                    }
                }
            } else {
                let off = OBJECT_HEADER_BYTES + (field.0 as i32) * 8;
                let store_v = extend_to_i64(fb, vmap[value]);
                fb.ins().store(MemFlags::trusted(), store_v, obj_v, off);
            }
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
