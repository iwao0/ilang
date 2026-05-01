//! ARC retain/release emit helpers used by the lowering passes.
//!
//! Each `emit_*` lowers to a runtime FFI call; the per-type variants
//! exist because each heap kind has different layout/size info to pass.

use cranelift::prelude::*;
use cranelift_codegen::ir::types::I64;
use cranelift_module::Module;
use ilang_ast::ExprKind;

use crate::env::LowerCtx;
use crate::ty::JitTy;

/// True when an expression "borrows" an existing heap reference rather
/// than producing a fresh one. Used to decide whether a let binding
/// (or a call argument) needs an extra retain to balance its own
/// scope-exit / callee release.
pub(crate) fn is_aliased_heap_source(kind: &ExprKind) -> bool {
    matches!(
        kind,
        ExprKind::Var(_) | ExprKind::Field { .. } | ExprKind::Index { .. } | ExprKind::This
    )
}

pub(crate) fn emit_retain_object(b: &mut FunctionBuilder, lc: &mut LowerCtx, ptr: Value) {
    let r = lc.module.declare_func_in_func(lc.retain_object_id, b.func);
    b.ins().call(r, &[ptr]);
}

pub(crate) fn emit_release_object(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    ptr: Value,
    class_id: u32,
) {
    let r = lc.module.declare_func_in_func(lc.release_object_id, b.func);
    let size = lc.class_layouts[class_id as usize].size as i64;
    let size_v = b.ins().iconst(I64, size);
    b.ins().call(r, &[ptr, size_v]);
}

pub(crate) fn emit_retain_string(b: &mut FunctionBuilder, lc: &mut LowerCtx, ptr: Value) {
    let r = lc.module.declare_func_in_func(lc.strfns.retain, b.func);
    b.ins().call(r, &[ptr]);
}

pub(crate) fn emit_release_string(b: &mut FunctionBuilder, lc: &mut LowerCtx, ptr: Value) {
    let r = lc.module.declare_func_in_func(lc.strfns.release, b.func);
    b.ins().call(r, &[ptr]);
}

pub(crate) fn emit_retain_array(b: &mut FunctionBuilder, lc: &mut LowerCtx, ptr: Value) {
    let r = lc.module.declare_func_in_func(lc.arrfns.retain, b.func);
    b.ins().call(r, &[ptr]);
}

pub(crate) fn emit_release_array(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    ptr: Value,
    array_id: u32,
) {
    let r = lc.module.declare_func_in_func(lc.arrfns.release, b.func);
    let elem_size = lc.array_kinds[array_id as usize].elem.size_bytes() as i64;
    let size_v = b.ins().iconst(I64, elem_size);
    b.ins().call(r, &[ptr, size_v]);
}

/// Emit retain for any heap-typed value. No-op for non-heap types.
/// `Optional<inner>` dispatches to inner's retain (the runtime helpers
/// already guard against null pointers).
pub(crate) fn emit_retain_heap(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    ptr: Value,
    ty: JitTy,
) {
    match ty {
        JitTy::Object(_) => emit_retain_object(b, lc, ptr),
        JitTy::Str => emit_retain_string(b, lc, ptr),
        JitTy::Array(_) => emit_retain_array(b, lc, ptr),
        JitTy::Optional(id) => {
            let inner = lc.optional_inners[id as usize];
            emit_retain_heap(b, lc, ptr, inner);
        }
        _ => {}
    }
}

/// Emit release for any heap-typed value. No-op for non-heap types.
pub(crate) fn emit_release_heap(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    ptr: Value,
    ty: JitTy,
) {
    match ty {
        JitTy::Object(id) => emit_release_object(b, lc, ptr, id),
        JitTy::Str => emit_release_string(b, lc, ptr),
        JitTy::Array(id) => emit_release_array(b, lc, ptr, id),
        JitTy::Optional(id) => {
            let inner = lc.optional_inners[id as usize];
            emit_release_heap(b, lc, ptr, inner);
        }
        _ => {}
    }
}
