//! `console.log` value-printer and panic-guard emitters. Both walk
//! the runtime print helpers / `__ilang_panic` declared by the
//! caller and threaded through the `PrintIds` / `PanicAux` aggregates.

use cranelift::prelude::*;
use cranelift_codegen::ir::InstBuilder;
use cranelift_frontend::FunctionBuilder as ClifFnBuilder;
use cranelift_module::{DataId, Module};

use ilang_mir::MirTy;

use super::abi::{elem_byte_stride, elem_clif_type, extend_to_i64, reduce_from_i64};
use super::{PrintIds, PrintLits};

/// Print a literal C-string (DataId) via `__print_str`.
pub(super) fn emit_print_lit<M: Module>(
    fb: &mut ClifFnBuilder,
    module: &mut M,
    print_str: cranelift_module::FuncId,
    msg_data: DataId,
) {
    let gv = module.declare_data_in_func(msg_data, fb.func);
    let base = fb.ins().symbol_value(types::I64, gv);
    let off = fb.ins().iconst(types::I64, 24);
    let addr = fb.ins().iadd(base, off);
    let fr = module.declare_func_in_func(print_str, fb.func);
    fb.ins().call(fr, &[addr]);
}

/// Emit code that prints `value` of static type `ty`. Recurses into
/// composite types (Optional, Tuple, Array). For Map/Object/Closure/
/// Weak/Enum we fall back to printing the raw pointer (limited).
pub(super) fn emit_print_value<M: Module>(
    fb: &mut ClifFnBuilder,
    module: &mut M,
    print_ids: PrintIds,
    print_lits: PrintLits,
    ty: &MirTy,
    av: Value,
    enum_global: &[u32],
    class_struct_global: &[i64],
) {
    match ty {
        MirTy::Bool => {
            let v = if fb.func.dfg.value_type(av) == types::I64 {
                av
            } else {
                fb.ins().uextend(types::I64, av)
            };
            let r = module.declare_func_in_func(print_ids.bool_, fb.func);
            fb.ins().call(r, &[v]);
        }
        t if t.is_int() => {
            let v = if fb.func.dfg.value_type(av) == types::I64 {
                av
            } else if t.is_signed_int() {
                fb.ins().sextend(types::I64, av)
            } else {
                fb.ins().uextend(types::I64, av)
            };
            let r = module.declare_func_in_func(print_ids.int, fb.func);
            fb.ins().call(r, &[v]);
        }
        MirTy::F32 => {
            let v = fb.ins().fpromote(types::F64, av);
            let r = module.declare_func_in_func(print_ids.f64_, fb.func);
            fb.ins().call(r, &[v]);
        }
        MirTy::F64 => {
            let r = module.declare_func_in_func(print_ids.f64_, fb.func);
            fb.ins().call(r, &[av]);
        }
        MirTy::Str => {
            let r = module.declare_func_in_func(print_ids.str_, fb.func);
            fb.ins().call(r, &[av]);
        }
        MirTy::Optional(inner) => {
            // if av == 0 { print "none" } else { print "some(<inner>)" }
            let zero = fb.ins().iconst(types::I64, 0);
            let is_none = fb.ins().icmp(IntCC::Equal, av, zero);
            let none_blk = fb.create_block();
            let some_blk = fb.create_block();
            let cont_blk = fb.create_block();
            fb.ins().brif(is_none, none_blk, &[], some_blk, &[]);

            fb.switch_to_block(none_blk);
            fb.seal_block(none_blk);
            emit_print_lit(fb, module, print_ids.str_, print_lits.none);
            fb.ins().jump(cont_blk, [].iter());

            fb.switch_to_block(some_blk);
            fb.seal_block(some_blk);
            emit_print_lit(fb, module, print_ids.str_, print_lits.some_open);
            // Load the boxed inner value (the some payload is a 1-cell heap).
            let raw = fb.ins().load(types::I64, MemFlags::trusted(), av, 0);
            let inner_v = reduce_from_i64(fb, inner, raw);
            emit_print_value(fb, module, print_ids, print_lits, inner, inner_v, enum_global, class_struct_global);
            emit_print_lit(fb, module, print_ids.str_, print_lits.close_paren);
            fb.ins().jump(cont_blk, [].iter());

            fb.switch_to_block(cont_blk);
            fb.seal_block(cont_blk);
        }
        MirTy::Tuple(items) => {
            emit_print_lit(fb, module, print_ids.str_, print_lits.open_paren);
            for (i, ity) in items.iter().enumerate() {
                if i > 0 {
                    emit_print_lit(fb, module, print_ids.str_, print_lits.comma_sp);
                }
                let off = (i as i32) * 8;
                let raw = fb.ins().load(types::I64, MemFlags::trusted(), av, off);
                let elem_v = reduce_from_i64(fb, ity, raw);
                emit_print_value(fb, module, print_ids, print_lits, ity, elem_v, enum_global, class_struct_global);
            }
            emit_print_lit(fb, module, print_ids.str_, print_lits.close_paren);
        }
        MirTy::Array { elem, len: arr_len } => {
            // Dynamic arrays carry a `[len|cap|data_ptr|..]` header;
            // fixed-length `T[N]` arrays are header-less inline storage
            // — the value points straight at the elements and the
            // length is the static `N`.
            let (len, data_ptr) = match arr_len {
                Some(n) => (fb.ins().iconst(types::I64, *n as i64), av),
                None => (
                    fb.ins().load(types::I64, MemFlags::trusted(), av, 0),
                    fb.ins().load(types::I64, MemFlags::trusted(), av, 16),
                ),
            };
            emit_print_lit(fb, module, print_ids.str_, print_lits.open_bracket);
            // for i in 0..len: print elem; if i+1 < len: print ", "
            let header = fb.create_block();
            fb.append_block_param(header, types::I64);
            let body_blk = fb.create_block();
            let exit_blk = fb.create_block();

            let zero = fb.ins().iconst(types::I64, 0);
            fb.ins().jump(header, [zero.into()].iter());

            fb.switch_to_block(header);
            let i_arg = fb.block_params(header)[0];
            let cond = fb.ins().icmp(IntCC::SignedLessThan, i_arg, len);
            fb.ins().brif(cond, body_blk, &[], exit_blk, &[]);

            fb.switch_to_block(body_blk);
            fb.seal_block(body_blk);
            // Print separator if i > 0.
            let zero2 = fb.ins().iconst(types::I64, 0);
            let is_first = fb.ins().icmp(IntCC::Equal, i_arg, zero2);
            let print_sep = fb.create_block();
            let after_sep = fb.create_block();
            fb.ins().brif(is_first, after_sep, &[], print_sep, &[]);

            fb.switch_to_block(print_sep);
            fb.seal_block(print_sep);
            emit_print_lit(fb, module, print_ids.str_, print_lits.comma_sp);
            fb.ins().jump(after_sep, [].iter());

            fb.switch_to_block(after_sep);
            fb.seal_block(after_sep);
            // Load elem at i, honoring the element's packed stride.
            let stride = fb.ins().iconst(types::I64, elem_byte_stride(elem));
            let off = fb.ins().imul(i_arg, stride);
            let addr = fb.ins().iadd(data_ptr, off);
            let elem_v = match elem_clif_type(elem) {
                Some(ct) if ct == types::I8 => {
                    fb.ins().load(types::I8, MemFlags::trusted(), addr, 0)
                }
                Some(ct) if ct == types::I16 => {
                    fb.ins().load(types::I16, MemFlags::trusted(), addr, 0)
                }
                Some(ct) if ct == types::I32 => {
                    fb.ins().load(types::I32, MemFlags::trusted(), addr, 0)
                }
                Some(ct) if ct == types::F32 => {
                    fb.ins().load(types::F32, MemFlags::trusted(), addr, 0)
                }
                Some(ct) if ct == types::F64 => {
                    fb.ins().load(types::F64, MemFlags::trusted(), addr, 0)
                }
                _ => {
                    let raw = fb.ins().load(types::I64, MemFlags::trusted(), addr, 0);
                    reduce_from_i64(fb, elem, raw)
                }
            };
            emit_print_value(fb, module, print_ids, print_lits, elem, elem_v, enum_global, class_struct_global);
            // i = i + 1
            let one = fb.ins().iconst(types::I64, 1);
            let i_next = fb.ins().iadd(i_arg, one);
            fb.ins().jump(header, [i_next.into()].iter());

            fb.seal_block(header);
            fb.switch_to_block(exit_blk);
            fb.seal_block(exit_blk);
            emit_print_lit(fb, module, print_ids.str_, print_lits.close_bracket);
        }
        MirTy::Object(cid) => {
            let sg = class_struct_global
                .get(cid.0 as usize)
                .copied()
                .unwrap_or(-1);
            if sg >= 0 {
                let cid_v = fb.ins().iconst(types::I64, sg);
                let r = module.declare_func_in_func(print_ids.struct_, fb.func);
                fb.ins().call(r, &[cid_v, av]);
            } else {
                let r = module.declare_func_in_func(print_ids.object, fb.func);
                fb.ins().call(r, &[av]);
            }
        }
        MirTy::Fn(_) => {
            let r = module.declare_func_in_func(print_ids.fn_, fb.func);
            fb.ins().call(r, &[av]);
        }
        MirTy::Map { .. } => {
            let r = module.declare_func_in_func(print_ids.map, fb.func);
            fb.ins().call(r, &[av]);
        }
        MirTy::Set { .. } => {
            let r = module.declare_func_in_func(print_ids.set, fb.func);
            fb.ins().call(r, &[av]);
        }
        MirTy::Weak(_) => {
            let r = module.declare_func_in_func(print_ids.weak, fb.func);
            fb.ins().call(r, &[av]);
        }
        MirTy::Enum(eid) => {
            let global = enum_global[eid.0 as usize] as i64;
            let id_v = fb.ins().iconst(types::I64, global);
            let r = module.declare_func_in_func(print_ids.enum_, fb.func);
            fb.ins().call(r, &[id_v, av]);
        }
        _ => {
            // Fallback: print as raw int (pointer / tag).
            let v = extend_to_i64(fb, av);
            let r = module.declare_func_in_func(print_ids.int, fb.func);
            fb.ins().call(r, &[v]);
        }
    }
}

/// Emit `if cond_truthy { call __ilang_panic(msg); trap } else { fallthrough }`.
/// `cond` must be an i8 boolean (1 = panic). Used for div/0, OOB, unwrap-None.
pub(super) fn emit_panic_if<M: Module>(
    fb: &mut ClifFnBuilder,
    module: &mut M,
    panic_fn: cranelift_module::FuncId,
    msg_data: DataId,
    cond: Value,
) {
    let panic_block = fb.create_block();
    let cont_block = fb.create_block();
    fb.ins().brif(cond, panic_block, &[], cont_block, &[]);
    fb.switch_to_block(panic_block);
    fb.seal_block(panic_block);
    let gv = module.declare_data_in_func(msg_data, fb.func);
    let base = fb.ins().symbol_value(types::I64, gv);
    let off = fb.ins().iconst(types::I64, 24);
    let addr = fb.ins().iadd(base, off);
    let fr = module.declare_func_in_func(panic_fn, fb.func);
    fb.ins().call(fr, &[addr]);
    fb.ins().trap(TrapCode::user(1).unwrap());
    fb.switch_to_block(cont_block);
    fb.seal_block(cont_block);
}
