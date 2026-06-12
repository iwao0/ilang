//! ARC instruction lowering — `Retain`, `Release`. Routes per
//! `MirTy` to the matching `__retain_*` / `__release_*` runtime
//! helper; stack-promoted values, CRepr structs, COM handles, and
//! fixed-length arrays skip rc bookkeeping entirely.

#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::InstBuilder;
use cranelift_frontend::FunctionBuilder as ClifFnBuilder;
use cranelift_module::Module;

use ilang_mir::{Inst, MirTy, ValueId};

use super::super::CompileError;

pub(super) fn lower_arc_inst<M: Module>(
    fb: &mut ClifFnBuilder,
    vmap: &mut HashMap<ValueId, Value>,
    module: &mut M,
    prog_ctx: &super::super::ProgCtx,
    fn_ctx: &super::super::FnCtx,
    inst: &Inst,
) -> Result<(), CompileError> {
    let super::super::ProgCtx {
        prog, panic_aux, ..
    } = *prog_ctx;
    let super::super::FnCtx {
        stack_local, func, ..
    } = *fn_ctx;
    match inst {
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
                    // Mirror the Retain side: `@com interface` /
                    // `@handle pub struct` carry no ilang rc.
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
                        // backing buffer directly. Only emitted for
                        // Locals tagged in `crepr_owned_locals`.
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
                MirTy::Fn(_) => call_unary(fb, module, panic_aux.release_closure, vmap[value]),
                MirTy::Array { len, elem } => {
                    if let Some(n) = len {
                        // Fixed-length array. With ARC-pointer
                        // elements the OWNER's release drops each
                        // element's share and frees the header-less
                        // `n * 8` buffer (the lowerer only emits
                        // Release for owned bindings — aliases are
                        // PatternBinding borrows). Primitive / CRepr
                        // / handle elements keep the legacy no-op:
                        // their slots aren't ARC pointers and their
                        // stride isn't 8 (freeing `n * 8` would be
                        // the wrong size for e.g. `Vertex[3]`).
                        let ekind =
                            super::super::print_kind::kind_tag_of(elem, &prog.classes);
                        if ekind != 0 {
                            let ptr = vmap[value];
                            let len_v = fb.ins().iconst(types::I64, *n as i64);
                            let kind_v = fb.ins().iconst(types::I64, ekind);
                            let r = module
                                .declare_func_in_func(panic_aux.release_fixed_array, fb.func);
                            fb.ins().call(r, &[ptr, len_v, kind_v]);
                        }
                        return Ok(());
                    }
                    call_unary(fb, module, panic_aux.release_array, vmap[value]);
                }
                MirTy::Optional(_) => {
                    call_unary(fb, module, panic_aux.release_optional, vmap[value]);
                }
                MirTy::Tuple(_) => {
                    call_unary(fb, module, panic_aux.release_tuple, vmap[value]);
                }
                MirTy::Map { .. } => {
                    call_unary(fb, module, panic_aux.release_map, vmap[value]);
                }
                MirTy::Set { .. } => {
                    call_unary(fb, module, panic_aux.release_set, vmap[value]);
                }
                MirTy::Promise(_) => {
                    call_unary(fb, module, panic_aux.release_promise, vmap[value]);
                }
                MirTy::Str => {
                    call_unary(fb, module, panic_aux.release_string, vmap[value]);
                }
                MirTy::Enum(_) => {
                    call_unary(fb, module, panic_aux.release_enum, vmap[value]);
                }
                MirTy::Weak(_) => {
                    call_unary(fb, module, panic_aux.release_weak, vmap[value]);
                }
                _ => {}
            }
        }
        Inst::Retain { value } => {
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
                    // `@com interface` / `@handle pub struct` skip rc
                    // (see Release for details).
                    if layout.is_com_interface || layout.is_handle {
                        return Ok(());
                    }
                    call_unary(fb, module, panic_aux.retain_obj, vmap[value]);
                }
                MirTy::Fn(_) => call_unary(fb, module, panic_aux.retain_closure, vmap[value]),
                MirTy::Array { len, .. } => {
                    if len.is_some() {
                        return Ok(());
                    }
                    call_unary(fb, module, panic_aux.retain_array, vmap[value]);
                }
                MirTy::Optional(_) => {
                    call_unary(fb, module, panic_aux.retain_optional, vmap[value]);
                }
                MirTy::Tuple(_) => {
                    call_unary(fb, module, panic_aux.retain_tuple, vmap[value]);
                }
                MirTy::Map { .. } => {
                    call_unary(fb, module, panic_aux.retain_map, vmap[value]);
                }
                MirTy::Set { .. } => {
                    call_unary(fb, module, panic_aux.retain_set, vmap[value]);
                }
                MirTy::Promise(_) => {
                    call_unary(fb, module, panic_aux.retain_promise, vmap[value]);
                }
                MirTy::Str => {
                    call_unary(fb, module, panic_aux.retain_string, vmap[value]);
                }
                MirTy::Enum(_) => {
                    call_unary(fb, module, panic_aux.retain_enum, vmap[value]);
                }
                MirTy::Weak(_) => {
                    call_unary(fb, module, panic_aux.retain_weak, vmap[value]);
                }
                _ => {}
            }
        }
        _ => unreachable!("lower_arc_inst called with non-ARC inst"),
    }
    Ok(())
}

fn call_unary<M: Module>(
    fb: &mut ClifFnBuilder,
    module: &mut M,
    fid: cranelift_module::FuncId,
    arg: Value,
) {
    let r = module.declare_func_in_func(fid, fb.func);
    fb.ins().call(r, &[arg]);
}
