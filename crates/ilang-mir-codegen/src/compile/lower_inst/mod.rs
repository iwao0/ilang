//! Per-instruction MIR → cranelift lowering. The bulk of `compile/`
//! lives here: each `Inst` variant emits the cranelift sequence
//! that realises it, with the surrounding `BodyCx`-style state
//! threaded in as parameters from `lower_function`. Large variants
//! (Call / LoadField / StoreField) are split out into per-topic
//! submodules — they take the same long parameter list since
//! there isn't a dedicated context struct yet.

mod array;
mod call_dispatch;
mod calls;
mod objects;

use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::InstBuilder;
use cranelift_frontend::FunctionBuilder as ClifFnBuilder;
use cranelift_module::Module;

use ilang_mir::{BinOp, Inst, MirConst, MirTy, UnOp, ValueId};

use crate::ty::mir_to_clif;

use super::abi::{extend_to_i64, reduce_from_i64};
use super::binop_cast::{lower_binop, lower_cast};
use super::lower_term_const::lower_const;
use super::print_emit::emit_panic_if;
use super::print_kind::{
    kind_tag_of, print_kind_id, KIND_ARRAY, KIND_CLOSURE, KIND_ENUM, KIND_MAP, KIND_NONE,
    KIND_OBJECT, KIND_OPTIONAL, KIND_PROMISE, KIND_STR, KIND_TUPLE,
};
use super::{emit_is_subclass, CompileError, OBJECT_HEADER_BYTES};

/// Inline byte-wise copy of `total` bytes from `src` to `dst_addr`.
/// Mirrors the pattern used in `objects.rs` for CRepr struct copies —
/// it avoids depending on the JIT's `memcpy` libcall resolution
/// (which can race with how mir-codegen declares its own symbols).
pub(super) fn crepr_struct_copy(fb: &mut ClifFnBuilder, src: Value, dst_addr: Value, total: i64) {
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
    vmap: &mut HashMap<ValueId, Value>,
    module: &mut M,
    prog_ctx: &super::ProgCtx,
    fn_ctx: &super::FnCtx,
    inst: &Inst,
) -> Result<(), CompileError> {
    let super::ProgCtx {
        fn_ids,
        static_data,
        string_data,
        alloc_id,
        map_ids,
        str_ids,
        panic_aux,
        prog,
        class_global,
        enum_global,
        ..
    } = *prog_ctx;
    let super::FnCtx {
        func,
        locals,
        local_slots,
        env_value,
        stack_local,
    } = *fn_ctx;
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
            calls::lower_call(fb, vmap, module, prog_ctx, fn_ctx, dst, callee, args)?;
        }
        Inst::VirtCall { .. }
        | Inst::CallIndirect { .. }
        | Inst::CallRawIndirect { .. }
        | Inst::ComCall { .. } => {
            call_dispatch::lower_call_dispatch_inst(fb, vmap, module, prog_ctx, fn_ctx, inst)?;
        }
        Inst::MakeClosure { dst, func: fid, captures } => {
            let cid = *fn_ids.get(fid).ok_or_else(|| {
                CompileError::Other(format!("missing fn id #{}", fid.0))
            })?;
            use super::layout::closure_header as ch;
            let local_ref = module.declare_func_in_func(cid, fb.func);
            let n_caps = captures.len() as i64;
            let bytes = fb.ins().iconst(types::I64, (2 + n_caps) * 8);
            let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
            let call = fb.ins().call(alloc_ref, &[bytes]);
            let ptr = fb.inst_results(call)[0];
            let fn_addr = fb.ins().func_addr(types::I64, local_ref);
            fb.ins().store(MemFlags::trusted(), fn_addr, ptr, ch::FN_ADDR);
            let one = fb.ins().iconst(types::I64, 1);
            fb.ins().store(MemFlags::trusted(), one, ptr, ch::RC);
            for (i, c) in captures.iter().enumerate() {
                let v_ext = extend_to_i64(fb, vmap[c]);
                fb.ins().store(
                    MemFlags::trusted(),
                    v_ext,
                    ptr,
                    ch::CAPTURE_BASE + (i as i32) * 8,
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
                MirTy::Set { .. } => {
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.release_set, fb.func);
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
                MirTy::Set { .. } => {
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.retain_set, fb.func);
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
            use super::layout::object_header as oh;
            let p = vmap[value];
            let cid = fb.ins().load(types::I64, MemFlags::trusted(), p, oh::CLASS_ID);
            vmap.insert(*dst, cid);
        }
        Inst::IsInstance { dst, value, class } => {
            use super::layout::object_header as oh;
            let p = vmap[value];
            let cid = fb.ins().load(types::I64, MemFlags::trusted(), p, oh::CLASS_ID);
            let v = emit_is_subclass(fb, cid, *class, prog, class_global);
            vmap.insert(*dst, v);
        }
        Inst::DowncastOrNone { dst, value, class } => {
            // `value as? Class` → some(value) if dynamic class is
            // a subtype of `class`, else none. Optional<Object> is
            // boxed: we emit NewOptional on the some-branch, 0 on the
            // none-branch, and merge through a block-arg.
            use super::layout::object_header as oh;
            let p = vmap[value];
            let cid = fb.ins().load(types::I64, MemFlags::trusted(), p, oh::CLASS_ID);
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
            use super::layout::object_header as oh;
            use super::layout::optional_header as opth;
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
            let rc = fb.ins().load(types::I64, MemFlags::trusted(), p, oh::RC);
            let alive = fb.ins().icmp_imm(IntCC::SignedGreaterThan, rc, 0);
            let alloc_blk = fb.create_block();
            fb.ins().brif(alive, alloc_blk, &[], none_blk, &[]);

            // alive: bump strong rc (caller now owns +1) and box into
            // a fresh Optional cell.
            fb.switch_to_block(alloc_blk);
            fb.seal_block(alloc_blk);
            let one = fb.ins().iconst(types::I64, 1);
            let new_rc = fb.ins().iadd(rc, one);
            fb.ins().store(MemFlags::trusted(), new_rc, p, oh::RC);
            let bytes = fb.ins().iconst(types::I64, opth::SIZE);
            let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
            let call = fb.ins().call(alloc_ref, &[bytes]);
            let cell = fb.inst_results(call)[0];
            fb.ins().store(MemFlags::trusted(), p, cell, opth::VALUE);
            fb.ins().store(MemFlags::trusted(), one, cell, opth::RC);
            let kind = fb.ins().iconst(types::I64, 1); // PrintKind::Object cascade
            fb.ins().store(MemFlags::trusted(), kind, cell, opth::KIND_TAG);
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
            use super::layout::object_header as oh;
            let cid_v = fb.ins().iconst(types::I64, class_global[class.0 as usize] as i64);
            fb.ins().store(MemFlags::trusted(), cid_v, ptr, oh::CLASS_ID);
            let one = fb.ins().iconst(types::I64, 1);
            fb.ins().store(MemFlags::trusted(), one, ptr, oh::RC);

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
        Inst::NewArray { .. }
        | Inst::NewArrayEmpty { .. }
        | Inst::NewSimd { .. }
        | Inst::ArrayLen { .. }
        | Inst::ArrayLoad { .. }
        | Inst::ArrayStore { .. } => {
            array::lower_array_inst(fb, vmap, module, prog_ctx, fn_ctx, inst)?;
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
            use super::layout::enum_header as eh;
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
                    eh::PAYLOAD_BASE + (i as i32) * 8,
                );
            }
            vmap.insert(*dst, ptr);
        }
        Inst::EnumTag { dst, value } => {
            use super::layout::enum_header as eh;
            let p = vmap[value];
            let v = fb.ins().load(types::I64, MemFlags::trusted(), p, eh::TAG);
            vmap.insert(*dst, v);
        }
        Inst::EnumDiscStr { dst, enum_id, value } => {
            // `enum-as-string` cast for `: string`-repr enums.
            // Load the box's tag (variant index), then call
            // `__enum_disc_str(global, tag)` to get a fresh
            // `StringRc *` with the variant's declared
            // discriminant string.
            use super::layout::enum_header as eh;
            let p = vmap[value];
            let tag = fb.ins().load(types::I64, MemFlags::trusted(), p, eh::TAG);
            let global = enum_global[enum_id.0 as usize] as i64;
            let global_v = fb.ins().iconst(types::I64, global);
            let f = module.declare_func_in_func(panic_aux.enum_disc_str, fb.func);
            let call = fb.ins().call(f, &[global_v, tag]);
            let v = fb.inst_results(call)[0];
            vmap.insert(*dst, v);
        }
        Inst::EnumPayload { dst, value, variant: _, idx } => {
            use super::layout::enum_header as eh;
            let p = vmap[value];
            let off = eh::PAYLOAD_BASE + (*idx as i32) * 8;
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
            //   base + RC      = rc
            //   base + PACKED  = packed:
            //                      bits  0-15 = arity (max 65535)
            //                      bits 16-63 = 4-bit KIND_* tag per
            //                                   element (up to 12;
            //                                   12+ leak heap content
            //                                   but the cell itself
            //                                   is still freed).
            //   base + ELEM_BASE = element 0 ← user_ptr
            // TupleExtract reads from offset 0 of user_ptr, unchanged.
            use super::layout::tuple_header as th;
            let n = items.len() as i64;
            let bytes = fb.ins().iconst(types::I64, th::ELEM_BASE as i64 + n.max(1) * 8);
            let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
            let call = fb.ins().call(alloc_ref, &[bytes]);
            let base = fb.inst_results(call)[0];
            let elem_off = fb.ins().iconst(types::I64, th::ELEM_BASE as i64);
            let ptr = fb.ins().iadd(base, elem_off);
            // rc = 1
            let one = fb.ins().iconst(types::I64, 1);
            fb.ins().store(MemFlags::trusted(), one, base, th::RC);
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
            fb.ins().store(MemFlags::trusted(), mask_v, base, th::PACKED);
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
            use super::layout::optional_header as opth;
            let bytes = fb.ins().iconst(types::I64, opth::SIZE);
            let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
            let call = fb.ins().call(alloc_ref, &[bytes]);
            let ptr = fb.inst_results(call)[0];
            let v_ext = extend_to_i64(fb, vmap[value]);
            fb.ins().store(MemFlags::trusted(), v_ext, ptr, opth::VALUE);
            let one = fb.ins().iconst(types::I64, 1);
            fb.ins().store(MemFlags::trusted(), one, ptr, opth::RC);
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
            fb.ins().store(MemFlags::trusted(), tag_v, ptr, opth::KIND_TAG);
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
            objects::lower_load_field(fb, vmap, module, prog_ctx, fn_ctx, dst, obj, field)?;
        }
        Inst::StoreField { obj, field, value } => {
            objects::lower_store_field(fb, vmap, module, prog_ctx, fn_ctx, obj, field, value)?;
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
