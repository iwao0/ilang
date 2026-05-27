//! Per-instruction MIR → cranelift lowering. The bulk of `compile/`
//! lives here: each `Inst` variant emits the cranelift sequence
//! that realises it, with the surrounding `BodyCx`-style state
//! threaded in as parameters from `lower_function`. Large variants
//! (Call / LoadField / StoreField) are split out into per-topic
//! submodules — they take the same long parameter list since
//! there isn't a dedicated context struct yet.

mod arc;
mod array;
mod call_dispatch;
mod calls;
mod closure;
mod enum_inst;
mod locals;
mod map_inst;
mod objects;
mod optional;
mod rtti;
mod static_slot;
mod tuple_inst;

use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::InstBuilder;
use cranelift_frontend::FunctionBuilder as ClifFnBuilder;
use cranelift_module::Module;

use ilang_mir::{BinOp, Inst, MirConst, MirTy, UnOp, ValueId};

use super::binop_cast::{lower_binop, lower_cast};
use super::lower_term_const::lower_const;
use super::print_emit::emit_panic_if;
use super::CompileError;

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
        string_data,
        str_ids,
        panic_aux,
        ..
    } = *prog_ctx;
    let super::FnCtx { func, .. } = *fn_ctx;
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
                // The user-visible string pointer skips the 24-byte
                // `[cap | rc | len]` prefix (see string_data layout
                // above).
                let off = fb.ins().iconst(types::I64, 24);
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
        Inst::MakeClosure { .. }
        | Inst::FuncAddr { .. }
        | Inst::LoadCapture { .. } => {
            closure::lower_closure_inst(fb, vmap, module, prog_ctx, fn_ctx, inst)?;
        }
        // ARC operations are stubbed in M1: refcount machinery
        // arrives once the runtime is wired. Treating them as no-ops
        // means programs leak heap allocations until then, which is
        Inst::Release { .. } | Inst::Retain { .. } => {
            arc::lower_arc_inst(fb, vmap, module, prog_ctx, fn_ctx, inst)?;
        }
        Inst::WeakRetain { .. }
        | Inst::WeakRelease { .. } => {}
        Inst::TypeOf { .. }
        | Inst::IsInstance { .. }
        | Inst::DowncastOrNone { .. }
        | Inst::WeakUpgrade { .. } => {
            rtti::lower_rtti_inst(fb, vmap, module, prog_ctx, fn_ctx, inst)?;
        }
        Inst::DefLocal { .. }
        | Inst::UseLocal { .. }
        | Inst::AddrOfLocal { .. }
        | Inst::AddrOfField { .. } => {
            locals::lower_local_inst(fb, vmap, module, prog_ctx, fn_ctx, inst)?;
        }
        Inst::NewObject { dst, class, init_args, init } => {
            objects::lower_new_object(fb, vmap, module, prog_ctx, fn_ctx, dst, class, init_args, init)?;
        }
        Inst::NewArray { .. }
        | Inst::NewArrayEmpty { .. }
        | Inst::NewSimd { .. }
        | Inst::ArrayLen { .. }
        | Inst::ArrayLoad { .. }
        | Inst::ArrayStore { .. } => {
            array::lower_array_inst(fb, vmap, module, prog_ctx, fn_ctx, inst)?;
        }
        Inst::NewMap { .. } | Inst::MapGet { .. } | Inst::MapSet { .. } => {
            map_inst::lower_map_inst(fb, vmap, module, prog_ctx, fn_ctx, inst)?;
        }
        Inst::NewEnum { .. }
        | Inst::EnumTag { .. }
        | Inst::EnumDiscStr { .. }
        | Inst::EnumPayload { .. } => {
            enum_inst::lower_enum_inst(fb, vmap, module, prog_ctx, fn_ctx, inst)?;
        }
        Inst::NewTuple { .. } | Inst::TupleExtract { .. } => {
            tuple_inst::lower_tuple_inst(fb, vmap, module, prog_ctx, fn_ctx, inst)?;
        }
        Inst::NewOptional { .. }
        | Inst::OptionalIsSome { .. }
        | Inst::OptionalUnwrap { .. } => {
            optional::lower_optional_inst(fb, vmap, module, prog_ctx, fn_ctx, inst)?;
        }
        Inst::LoadField { dst, obj, field } => {
            objects::lower_load_field(fb, vmap, module, prog_ctx, fn_ctx, dst, obj, field)?;
        }
        Inst::StoreField { obj, field, value } => {
            objects::lower_store_field(fb, vmap, module, prog_ctx, fn_ctx, obj, field, value)?;
        }
        Inst::LoadStatic { .. } | Inst::StoreStatic { .. } => {
            static_slot::lower_static_slot_inst(fb, vmap, module, prog_ctx, fn_ctx, inst)?;
        }
        _ => {
            return Err(CompileError::Unsupported(
                "MIR inst kind not yet wired in mir-codegen",
            ));
        }
    }
    Ok(())
}
