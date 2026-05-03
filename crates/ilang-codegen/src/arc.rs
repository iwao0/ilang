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

/// Apply rc adjustments at a heap-typed bind site (`let`, function arg,
/// field/index assign). Encapsulates the aliased-vs-fresh and the
/// strong→weak downgrade subcases so each call site doesn't need to
/// reason about rc transitions individually.
///
/// - Heap target, aliased source: retain (the source binding still
///   holds its +1, so the new slot needs its own).
/// - Heap target, fresh source same kind: consume the source's rc=1
///   (no-op).
/// - `Weak<C>` target, fresh `Object<C>` source: retain_weak +
///   release_strong — the strong rc=1 owned by the fresh value gets
///   released, and the weak slot takes a +1 weak. With no other
///   strong holders, the object's strong reaches 0 and its drop fires
///   immediately; storage stays alive while weak_rc > 0 so `get()`
///   returns none.
pub(crate) fn emit_bind_retain(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    value_kind: &ExprKind,
    vt: JitTy,
    bind_ty: JitTy,
    coerced: Value,
) {
    if !bind_ty.is_heap() {
        return;
    }
    let aliased = is_aliased_heap_source(value_kind);
    if let (JitTy::Weak(target_class), JitTy::Object(source_class)) = (bind_ty, vt) {
        if target_class == source_class {
            // Always retain_weak; for fresh sources, additionally
            // release the strong rc since no other binding owns it.
            emit_retain_weak(b, lc, coerced);
            if !aliased {
                emit_release_object(b, lc, coerced, target_class);
            }
            return;
        }
    }
    if aliased {
        emit_retain_heap(b, lc, coerced, bind_ty);
    }
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

pub(crate) fn emit_retain_weak(b: &mut FunctionBuilder, lc: &mut LowerCtx, ptr: Value) {
    let r = lc.module.declare_func_in_func(lc.retain_weak_id, b.func);
    b.ins().call(r, &[ptr]);
}

pub(crate) fn emit_release_weak(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    ptr: Value,
    class_id: u32,
) {
    let r = lc.module.declare_func_in_func(lc.release_weak_id, b.func);
    let user_size = lc.class_layouts[class_id as usize].size as i64;
    let size_v = b.ins().iconst(I64, user_size);
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
        JitTy::Weak(_) => emit_retain_weak(b, lc, ptr),
        JitTy::EnumHeap(_) => emit_retain_object(b, lc, ptr),
        JitTy::Map(_) => emit_retain_map(b, lc, ptr),
        // Tuples share the object header (alloc_object); retain via
        // the same i64-rc helper.
        JitTy::Tuple(_) => emit_retain_object(b, lc, ptr),
        // Closure structs have their own ARC helpers.
        JitTy::Fn(_) => {
            let r = lc.module.declare_func_in_func(lc.retain_closure_id, b.func);
            b.ins().call(r, &[ptr]);
        }
        JitTy::Optional(id) => {
            let inner = lc.optional_inners[id as usize];
            if inner.is_heap() {
                emit_retain_heap(b, lc, ptr, inner);
            } else {
                // Primitive Optional: box has its own rc.
                let r = lc.module.declare_func_in_func(lc.optional_box_retain_id, b.func);
                b.ins().call(r, &[ptr]);
            }
        }
        _ => {}
    }
}

pub(crate) fn emit_retain_map(b: &mut FunctionBuilder, lc: &mut LowerCtx, ptr: Value) {
    let r = lc.module.declare_func_in_func(lc.retain_map_id, b.func);
    b.ins().call(r, &[ptr]);
}

pub(crate) fn emit_release_map(b: &mut FunctionBuilder, lc: &mut LowerCtx, ptr: Value) {
    let r = lc.module.declare_func_in_func(lc.release_map_id, b.func);
    b.ins().call(r, &[ptr]);
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
        JitTy::Weak(class_id) => emit_release_weak(b, lc, ptr, class_id),
        JitTy::EnumHeap(enum_id) => emit_release_enum_heap(b, lc, ptr, enum_id),
        JitTy::Map(_) => emit_release_map(b, lc, ptr),
        JitTy::Tuple(tuple_id) => {
            let size = lc.tuple_kinds[tuple_id as usize].size as i64;
            let size_v = b.ins().iconst(I64, size);
            let r = lc.module.declare_func_in_func(lc.release_object_id, b.func);
            b.ins().call(r, &[ptr, size_v]);
        }
        JitTy::Fn(_) => {
            let r = lc
                .module
                .declare_func_in_func(lc.release_closure_id, b.func);
            b.ins().call(r, &[ptr]);
        }
        JitTy::Optional(id) => {
            let inner = lc.optional_inners[id as usize];
            if inner.is_heap() {
                emit_release_heap(b, lc, ptr, inner);
            } else {
                // Primitive Optional: free the box on rc=0.
                let size = inner.size_bytes() as i64;
                let size_v = b.ins().iconst(I64, size);
                let r = lc.module.declare_func_in_func(lc.optional_box_release_id, b.func);
                b.ins().call(r, &[ptr, size_v]);
            }
        }
        _ => {}
    }
}

/// Release an enum-heap value. The user_size passed to release_object
/// is the per-enum tagged-union total: 8 (tag area) + max_payload_size.
pub(crate) fn emit_release_enum_heap(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    ptr: Value,
    enum_id: u32,
) {
    let layout = &lc.enum_layouts[enum_id as usize];
    let user_size = 8i64 + layout.max_payload_size as i64;
    let r = lc.module.declare_func_in_func(lc.release_object_id, b.func);
    let size_v = b.ins().iconst(I64, user_size);
    b.ins().call(r, &[ptr, size_v]);
}
