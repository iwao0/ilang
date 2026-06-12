//! Value-to-string emitter for backtick template literals. Parallel
//! to `print_emit::emit_print_value` but builds an ilang string and
//! returns its pointer instead of writing to stdout. Composite types
//! (Optional / Tuple / Array) are recursively unrolled so the result
//! matches the `console.log` formatting; terminal kinds bottom out in
//! `$fmt.*` runtime helpers declared in [`FmtIds`].

use cranelift::prelude::*;
use cranelift_codegen::ir::InstBuilder;
use cranelift_frontend::FunctionBuilder as ClifFnBuilder;
use cranelift_module::{DataId, Module};

use ilang_mir::MirTy;

use super::abi::{elem_byte_stride, elem_clif_type, extend_to_i64, reduce_from_i64};
use super::{FmtIds, PrintLits, StrIds};

/// Materialize a literal C-string symbol as a ilang string pointer
/// (i.e. step past the 24-byte `[cap | rc | len]` prefix). Mirrors
/// the bump applied to string `Const`s in the regular MIR lowering.
fn lit_value<M: Module>(
    fb: &mut ClifFnBuilder,
    module: &mut M,
    data: DataId,
) -> Value {
    let gv = module.declare_data_in_func(data, fb.func);
    let base = fb.ins().symbol_value(types::I64, gv);
    let off = fb.ins().iconst(types::I64, 24);
    fb.ins().iadd(base, off)
}

fn concat<M: Module>(
    fb: &mut ClifFnBuilder,
    module: &mut M,
    str_ids: StrIds,
    a: Value,
    b: Value,
) -> Value {
    let r = module.declare_func_in_func(str_ids.concat, fb.func);
    let call = fb.ins().call(r, &[a, b]);
    fb.inst_results(call)[0]
}

pub(super) fn emit_format_value<M: Module>(
    fb: &mut ClifFnBuilder,
    module: &mut M,
    str_ids: StrIds,
    fmt_ids: FmtIds,
    print_lits: PrintLits,
    ty: &MirTy,
    av: Value,
    enum_global: &[u32],
    class_struct_global: &[i64],
    classes: &[ilang_mir::ClassLayout],
) -> Value {
    match ty {
        MirTy::Bool => {
            let v = if fb.func.dfg.value_type(av) == types::I64 {
                av
            } else {
                fb.ins().uextend(types::I64, av)
            };
            let r = module.declare_func_in_func(fmt_ids.bool_, fb.func);
            let call = fb.ins().call(r, &[v]);
            fb.inst_results(call)[0]
        }
        t if t.is_int() => {
            let v = if fb.func.dfg.value_type(av) == types::I64 {
                av
            } else if t.is_signed_int() {
                fb.ins().sextend(types::I64, av)
            } else {
                fb.ins().uextend(types::I64, av)
            };
            let r = module.declare_func_in_func(fmt_ids.int, fb.func);
            let call = fb.ins().call(r, &[v]);
            fb.inst_results(call)[0]
        }
        MirTy::F32 => {
            let v = fb.ins().fpromote(types::F64, av);
            let r = module.declare_func_in_func(fmt_ids.f64_, fb.func);
            let call = fb.ins().call(r, &[v]);
            fb.inst_results(call)[0]
        }
        MirTy::F64 => {
            let r = module.declare_func_in_func(fmt_ids.f64_, fb.func);
            let call = fb.ins().call(r, &[av]);
            fb.inst_results(call)[0]
        }
        MirTy::Str => {
            let r = module.declare_func_in_func(fmt_ids.str_, fb.func);
            let call = fb.ins().call(r, &[av]);
            fb.inst_results(call)[0]
        }
        MirTy::Optional(inner) => {
            // if av == 0 { "none" } else { "some(" + inner + ")" }
            let zero = fb.ins().iconst(types::I64, 0);
            let is_none = fb.ins().icmp(IntCC::Equal, av, zero);
            let none_blk = fb.create_block();
            let some_blk = fb.create_block();
            let cont_blk = fb.create_block();
            fb.append_block_param(cont_blk, types::I64);
            fb.ins().brif(is_none, none_blk, &[], some_blk, &[]);

            fb.switch_to_block(none_blk);
            fb.seal_block(none_blk);
            let none_v = lit_value(fb, module, print_lits.none);
            let none_copy = {
                // Run through $fmt.str so the result lives in the
                // string registry just like the some-branch's
                // intermediate; uniform release behaviour downstream.
                let r = module.declare_func_in_func(fmt_ids.str_, fb.func);
                let call = fb.ins().call(r, &[none_v]);
                fb.inst_results(call)[0]
            };
            fb.ins().jump(cont_blk, [none_copy.into()].iter());

            fb.switch_to_block(some_blk);
            fb.seal_block(some_blk);
            let some_open = lit_value(fb, module, print_lits.some_open);
            let some_open_s = {
                let r = module.declare_func_in_func(fmt_ids.str_, fb.func);
                let call = fb.ins().call(r, &[some_open]);
                fb.inst_results(call)[0]
            };
            let raw = fb.ins().load(types::I64, MemFlags::trusted(), av, 0);
            let inner_v = reduce_from_i64(fb, inner, raw);
            let inner_s = emit_format_value(
                fb, module, str_ids, fmt_ids, print_lits, inner, inner_v,
                enum_global, class_struct_global, classes,
            );
            let close = lit_value(fb, module, print_lits.close_paren);
            let close_s = {
                let r = module.declare_func_in_func(fmt_ids.str_, fb.func);
                let call = fb.ins().call(r, &[close]);
                fb.inst_results(call)[0]
            };
            let with_inner = concat(fb, module, str_ids, some_open_s, inner_s);
            let full = concat(fb, module, str_ids, with_inner, close_s);
            fb.ins().jump(cont_blk, [full.into()].iter());

            fb.switch_to_block(cont_blk);
            fb.seal_block(cont_blk);
            fb.block_params(cont_blk)[0]
        }
        MirTy::Tuple(items) => {
            let open = lit_value(fb, module, print_lits.open_paren);
            let mut acc = {
                let r = module.declare_func_in_func(fmt_ids.str_, fb.func);
                let call = fb.ins().call(r, &[open]);
                fb.inst_results(call)[0]
            };
            for (i, ity) in items.iter().enumerate() {
                if i > 0 {
                    let sep = lit_value(fb, module, print_lits.comma_sp);
                    acc = concat(fb, module, str_ids, acc, sep);
                }
                let off = (i as i32) * 8;
                let raw = fb.ins().load(types::I64, MemFlags::trusted(), av, off);
                let elem_v = reduce_from_i64(fb, ity, raw);
                let elem_s = emit_format_value(
                    fb, module, str_ids, fmt_ids, print_lits, ity, elem_v,
                    enum_global, class_struct_global, classes,
                );
                acc = concat(fb, module, str_ids, acc, elem_s);
            }
            let close = lit_value(fb, module, print_lits.close_paren);
            concat(fb, module, str_ids, acc, close)
        }
        MirTy::Array { elem, len: arr_len } => {
            // Dynamic arrays carry a `[len|cap|data_ptr|..]` header and
            // loop over elements writing "[a, b, c]"; fixed-length
            // `T[N]` arrays are header-less inline storage — the value
            // points straight at the elements with a static length.
            // The accumulator is threaded as a block parameter so each
            // iteration appends to the previous string without a slot.
            // ARC-element fixed arrays are headered like dynamic
            // arrays (only the length is fixed, at the type level);
            // kind-0 fixed arrays are header-less inline data.
            let fixed_inline = matches!(arr_len, Some(_))
                && super::print_kind::kind_tag_of(elem, classes) == 0;
            let (len, data_ptr) = match (arr_len, fixed_inline) {
                (Some(n), true) => (fb.ins().iconst(types::I64, *n as i64), av),
                _ => (
                    fb.ins().load(types::I64, MemFlags::trusted(), av, 0),
                    fb.ins().load(types::I64, MemFlags::trusted(), av, 16),
                ),
            };
            let open = lit_value(fb, module, print_lits.open_bracket);
            let open_s = {
                let r = module.declare_func_in_func(fmt_ids.str_, fb.func);
                let call = fb.ins().call(r, &[open]);
                fb.inst_results(call)[0]
            };

            let header = fb.create_block();
            fb.append_block_param(header, types::I64); // i
            fb.append_block_param(header, types::I64); // acc
            let body_blk = fb.create_block();
            let exit_blk = fb.create_block();
            fb.append_block_param(exit_blk, types::I64); // final acc

            let zero = fb.ins().iconst(types::I64, 0);
            fb.ins().jump(header, [zero.into(), open_s.into()].iter());

            fb.switch_to_block(header);
            let i_arg = fb.block_params(header)[0];
            let acc_arg = fb.block_params(header)[1];
            let cond = fb.ins().icmp(IntCC::SignedLessThan, i_arg, len);
            fb.ins().brif(cond, body_blk, &[], exit_blk, &[acc_arg.into()]);

            fb.switch_to_block(body_blk);
            fb.seal_block(body_blk);
            let zero2 = fb.ins().iconst(types::I64, 0);
            let is_first = fb.ins().icmp(IntCC::Equal, i_arg, zero2);
            let sep_blk = fb.create_block();
            let after_sep = fb.create_block();
            fb.append_block_param(after_sep, types::I64); // acc after sep
            fb.ins().brif(is_first, after_sep, &[acc_arg.into()], sep_blk, &[]);

            fb.switch_to_block(sep_blk);
            fb.seal_block(sep_blk);
            let sep = lit_value(fb, module, print_lits.comma_sp);
            let with_sep = concat(fb, module, str_ids, acc_arg, sep);
            fb.ins().jump(after_sep, [with_sep.into()].iter());

            fb.switch_to_block(after_sep);
            fb.seal_block(after_sep);
            let acc_now = fb.block_params(after_sep)[0];
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
            let elem_s = emit_format_value(
                fb, module, str_ids, fmt_ids, print_lits, elem, elem_v,
                enum_global, class_struct_global, classes,
            );
            let next_acc = concat(fb, module, str_ids, acc_now, elem_s);
            let one = fb.ins().iconst(types::I64, 1);
            let i_next = fb.ins().iadd(i_arg, one);
            fb.ins().jump(header, [i_next.into(), next_acc.into()].iter());

            fb.seal_block(header);
            fb.switch_to_block(exit_blk);
            fb.seal_block(exit_blk);
            let acc_final = fb.block_params(exit_blk)[0];
            let close = lit_value(fb, module, print_lits.close_bracket);
            concat(fb, module, str_ids, acc_final, close)
        }
        MirTy::Object(cid) => {
            let sg = class_struct_global
                .get(cid.0 as usize)
                .copied()
                .unwrap_or(-1);
            if sg >= 0 {
                let cid_v = fb.ins().iconst(types::I64, sg);
                let r = module.declare_func_in_func(fmt_ids.struct_, fb.func);
                let call = fb.ins().call(r, &[cid_v, av]);
                fb.inst_results(call)[0]
            } else {
                let r = module.declare_func_in_func(fmt_ids.object, fb.func);
                let call = fb.ins().call(r, &[av]);
                fb.inst_results(call)[0]
            }
        }
        MirTy::Fn(_) => {
            let r = module.declare_func_in_func(fmt_ids.fn_, fb.func);
            let call = fb.ins().call(r, &[av]);
            fb.inst_results(call)[0]
        }
        MirTy::Map { .. } => {
            let r = module.declare_func_in_func(fmt_ids.map, fb.func);
            let call = fb.ins().call(r, &[av]);
            fb.inst_results(call)[0]
        }
        MirTy::Set { .. } => {
            let r = module.declare_func_in_func(fmt_ids.set, fb.func);
            let call = fb.ins().call(r, &[av]);
            fb.inst_results(call)[0]
        }
        MirTy::Weak(_) => {
            let r = module.declare_func_in_func(fmt_ids.weak, fb.func);
            let call = fb.ins().call(r, &[av]);
            fb.inst_results(call)[0]
        }
        MirTy::Promise(_) => {
            let r = module.declare_func_in_func(fmt_ids.promise, fb.func);
            let call = fb.ins().call(r, &[av]);
            fb.inst_results(call)[0]
        }
        MirTy::Enum(eid) => {
            let global = enum_global[eid.0 as usize] as i64;
            let id_v = fb.ins().iconst(types::I64, global);
            let r = module.declare_func_in_func(fmt_ids.enum_, fb.func);
            let call = fb.ins().call(r, &[id_v, av]);
            fb.inst_results(call)[0]
        }
        _ => {
            // Fallback: format as raw int (pointer / tag) so we never
            // synthesise an invalid call. Matches `emit_print_value`'s
            // default arm.
            let v = extend_to_i64(fb, av);
            let r = module.declare_func_in_func(fmt_ids.int, fb.func);
            let call = fb.ins().call(r, &[v]);
            fb.inst_results(call)[0]
        }
    }
}
