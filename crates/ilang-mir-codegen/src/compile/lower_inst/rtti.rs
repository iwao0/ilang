//! RTTI / dynamic-type instruction lowering — `TypeOf`,
//! `IsInstance`, `DowncastOrNone`, `WeakUpgrade`. All four read the
//! object header's class_id / rc fields to make a runtime type
//! decision.

#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::InstBuilder;
use cranelift_frontend::FunctionBuilder as ClifFnBuilder;
use cranelift_module::Module;

use ilang_mir::{Inst, ValueId};

use super::super::{emit_is_subclass, CompileError};

pub(super) fn lower_rtti_inst<M: Module>(
    fb: &mut ClifFnBuilder,
    vmap: &mut HashMap<ValueId, Value>,
    module: &mut M,
    prog_ctx: &super::super::ProgCtx,
    fn_ctx: &super::super::FnCtx,
    inst: &Inst,
) -> Result<(), CompileError> {
    let super::super::ProgCtx {
        alloc_id,
        prog,
        class_global,
        ..
    } = *prog_ctx;
    let super::super::FnCtx { func, .. } = *fn_ctx;
    use ilang_mir::MirTy;
    use super::super::layout::object_header as oh;
    match inst {
        Inst::TypeOf { dst, value } => {
            // Object / Weak values carry a dynamic class id in their
            // header. Everything else (primitives, arrays, enums,
            // optionals, ...) is keyed by the static MirTy — return
            // the matching virtual id so `__class_name` / `__type_kind`
            // can still answer.
            let value_ty = func.ty_of(*value).clone();
            match &value_ty {
                MirTy::Object(_) | MirTy::Weak(_) => {
                    let p = vmap[value];
                    let cid = fb.ins().load(
                        types::I64,
                        MemFlags::trusted(),
                        p,
                        oh::CLASS_ID,
                    );
                    vmap.insert(*dst, cid);
                }
                _ => {
                    let id = crate::compile::mir_ty_to_type_id(
                        &value_ty,
                        &|c| class_global[c as usize],
                    );
                    let v = fb.ins().iconst(types::I64, id);
                    vmap.insert(*dst, v);
                }
            }
        }
        Inst::IsInstance { dst, value, class } => {
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
            // Box into a regular Optional cell [value | rc=1 |
            // kind_tag=Object] — mirrors WeakUpgrade. The cell owns
            // its own +1 of the value (rc bumped here) so releasing
            // the Optional cascades into the object. The old shape
            // was a bare 8-byte [value] cell with no rc / kind tag:
            // nothing ever released it (8 bytes leaked per `as?`
            // hit), and a release would have read past the alloc.
            use super::super::layout::optional_header as opth;
            let rc_old = fb.ins().load(types::I64, MemFlags::trusted(), p, oh::RC);
            let one = fb.ins().iconst(types::I64, 1);
            let rc_new = fb.ins().iadd(rc_old, one);
            fb.ins().store(MemFlags::trusted(), rc_new, p, oh::RC);
            let bytes = fb.ins().iconst(types::I64, opth::SIZE);
            let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
            let call = fb.ins().call(alloc_ref, &[bytes]);
            let ptr = fb.inst_results(call)[0];
            fb.ins().store(MemFlags::trusted(), p, ptr, opth::VALUE);
            fb.ins().store(MemFlags::trusted(), one, ptr, opth::RC);
            let kind = fb.ins().iconst(types::I64, 1); // KIND_OBJECT cascade
            fb.ins().store(MemFlags::trusted(), kind, ptr, opth::KIND_TAG);
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
            use super::super::layout::optional_header as opth;
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
        _ => unreachable!("lower_rtti_inst called with non-RTTI inst"),
    }
    Ok(())
}
