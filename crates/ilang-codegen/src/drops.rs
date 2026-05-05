//! JIT-generated drop wrappers (Phase D).
//!
//! Each non-trivial class gets a `__drop_<C>(this)` function that runs
//! the user `deinit` (if any) and recursively releases each heap-typed
//! field. Each non-trivial array kind gets a `__drop_arr_<id>(header)`
//! function that loops over the elements and releases each.
//!
//! The runtime `release_object` / `release_array` decrements the rc and,
//! on zero, calls the wrapper before deallocating storage. Trivial
//! classes / non-heap arrays use a 0 wrapper pointer, which the runtime
//! treats as "skip".

use cranelift::prelude::*;
use cranelift_codegen::ir::types::I64;
use cranelift_jit::JITModule;
use cranelift_module::{FuncId, Linkage, Module};

use crate::compiler::JitCompiler;
use crate::env::LowerCtx;
use crate::error::CodegenError;
use crate::ty::{
    ArrayKind, ClassLayout, EnumVariantLayout, JitTy, ENUM_PAYLOAD_OFFSET, ENUM_TAG_OFFSET,
};



/// True when `(outer_class, fty)` is an embedded `@repr(C)`
/// struct — the inner's bytes live inline in the outer's
/// allocation, so it must NOT be ARC-released as a heap pointer.
fn is_embedded_repr_c_field(
    compiler: &JitCompiler,
    outer_class_id: u32,
    fty: JitTy,
) -> bool {
    if !compiler.class_layouts[outer_class_id as usize].is_repr_c {
        return false;
    }
    match fty {
        JitTy::Object(inner_id) => {
            compiler.class_layouts[inner_id as usize].is_repr_c
        }
        _ => false,
    }
}

/// True when a class needs a drop wrapper at all (heap field or deinit).
fn class_needs_drop(compiler: &JitCompiler, class_id: u32) -> bool {
    let layout = &compiler.class_layouts[class_id as usize];
    let has_heap_field = layout.fields.values().any(|(_, fty)| {
        fty.is_heap() && !is_embedded_repr_c_field(compiler, class_id, *fty)
    });
    let has_deinit = compiler.class_methods[class_id as usize].contains_key("deinit");
    has_heap_field || has_deinit
}

/// Declare the drop wrapper FuncId for every class. Defining the body
/// happens later (after the user methods are defined) so the wrapper
/// can reference deinit by FuncId.
pub(crate) fn declare_class_drops(compiler: &mut JitCompiler) -> Result<(), CodegenError> {
    let n = compiler.class_layouts.len();
    compiler.class_drops = vec![None; n];
    for i in 0..n {
        if !class_needs_drop(compiler, i as u32) {
            continue;
        }
        let symbol = format!("__drop_{}", compiler.class_layouts[i].name);
        let id = declare_drop_fn(&mut compiler.module, &symbol)?;
        compiler.class_drops[i] = Some(id);
    }
    Ok(())
}

/// Define every class drop body. Call after all user methods are
/// defined.
pub(crate) fn define_class_drops(compiler: &mut JitCompiler) -> Result<(), CodegenError> {
    for i in 0..compiler.class_layouts.len() {
        if let Some(drop_id) = compiler.class_drops[i] {
            define_one_class_drop(compiler, i as u32, drop_id)?;
        }
    }
    Ok(())
}

fn define_one_class_drop(
    compiler: &mut JitCompiler,
    class_id: u32,
    drop_id: FuncId,
) -> Result<(), CodegenError> {
    compiler.module.clear_context(&mut compiler.ctx);
    compiler.ctx.func.signature =
        compiler.module.declarations().get_function_decl(drop_id).signature.clone();

    // Snapshot before constructing FunctionBuilder so we don't fight
    // borrow-check while mutating compiler.ctx.
    let deinit_fid = compiler.class_methods[class_id as usize]
        .get("deinit")
        .map(|m| m.id);
    let heap_fields: Vec<(u32, JitTy)> = compiler.class_layouts[class_id as usize]
        .fields
        .values()
        .filter(|(_, fty)| {
            fty.is_heap() && !is_embedded_repr_c_field(compiler, class_id, *fty)
        })
        .copied()
        .collect();

    let JitCompiler {
        module,
        ctx,
        builder_ctx,
        class_layouts,
        array_kinds,
        optional_inners,
        release_object_id,
        release_string_id,
        release_array_id,
        release_weak_id,
        release_map_id,
        optional_box_release_id,
        ..
    } = compiler;

    let mut builder = FunctionBuilder::new(&mut ctx.func, builder_ctx);
    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);
    builder.seal_block(entry);
    let this = builder.block_params(entry)[0];

    // 1) Call user deinit. The user's deinit has its own `this` exit-
    //    release suppressed (define_function_body checks f.name ==
    //    "deinit"), so the rc stays at 0 across the call.
    if let Some(fid) = deinit_fid {
        let func_ref = module.declare_func_in_func(fid, builder.func);
        builder.ins().call(func_ref, &[this]);
    }

    // 2) Release each heap-typed field.
    for (offset, fty) in heap_fields {
        let v = builder
            .ins()
            .load(I64, MemFlags::trusted(), this, offset as i32);
        emit_release_for(
            module,
            class_layouts,
            array_kinds,
            optional_inners,
            *release_object_id,
            *release_string_id,
            *release_array_id,
            *release_weak_id,
            *release_map_id,
            *optional_box_release_id,
            &mut builder,
            v,
            fty,
        );
    }

    // The wrapper does NOT release `this` — the runtime release_object
    // owns the rc=0 lifecycle and deallocs after we return.
    builder.ins().return_(&[]);
    builder.finalize();

    compiler
        .module
        .define_function(drop_id, &mut compiler.ctx)
        .map_err(|e| CodegenError::Module(e.to_string()))?;
    Ok(())
}

/// Lazy lookup for the per-tuple-kind drop wrapper. Returns the
/// fn-pointer Value to embed in the tuple's `alloc_object` call.
/// Returns iconst 0 when no element is heap (the runtime treats 0
/// as "skip the drop call").
pub(crate) fn tuple_drop_fn_ptr(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    tuple_id: u32,
) -> Value {
    let kind = lc.tuple_kinds[tuple_id as usize].clone();
    let any_heap = kind.elems.iter().any(|t| t.is_heap());
    if !any_heap {
        lc.tuple_drops.entry(tuple_id).or_insert(None);
        return b.ins().iconst(I64, 0);
    }
    let id = if let Some(Some(id)) = lc.tuple_drops.get(&tuple_id) {
        *id
    } else {
        let symbol = format!("__drop_tuple_{tuple_id}");
        let id = declare_drop_fn(lc.module, &symbol).expect("declare tuple drop");
        lc.tuple_drops.insert(tuple_id, Some(id));
        id
    };
    let func_ref = lc.module.declare_func_in_func(id, b.func);
    b.ins().func_addr(I64, func_ref)
}

/// Define every tuple drop body declared during lowering. Each body
/// loads each heap element from its offset and calls the appropriate
/// release helper. Mirrors `define_one_class_drop` in shape.
pub(crate) fn define_tuple_drops(compiler: &mut JitCompiler) -> Result<(), CodegenError> {
    let to_define: Vec<(u32, FuncId)> = compiler
        .tuple_drops
        .iter()
        .filter_map(|(k, v)| v.map(|id| (*k, id)))
        .collect();
    for (tuple_id, drop_id) in to_define {
        define_one_tuple_drop(compiler, tuple_id, drop_id)?;
    }
    Ok(())
}

fn define_one_tuple_drop(
    compiler: &mut JitCompiler,
    tuple_id: u32,
    drop_id: FuncId,
) -> Result<(), CodegenError> {
    compiler.module.clear_context(&mut compiler.ctx);
    compiler.ctx.func.signature =
        compiler.module.declarations().get_function_decl(drop_id).signature.clone();

    let kind = compiler.tuple_kinds[tuple_id as usize].clone();

    let JitCompiler {
        module,
        ctx,
        builder_ctx,
        class_layouts,
        array_kinds,
        optional_inners,
        release_object_id,
        release_string_id,
        release_array_id,
        release_weak_id,
        release_map_id,
        optional_box_release_id,
        ..
    } = compiler;

    let mut builder = FunctionBuilder::new(&mut ctx.func, builder_ctx);
    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);
    builder.seal_block(entry);
    let this = builder.block_params(entry)[0];

    for (i, &elem_ty) in kind.elems.iter().enumerate() {
        if !elem_ty.is_heap() {
            continue;
        }
        let cl = elem_ty.cl().expect("non-unit tuple element");
        let off = kind.offsets[i] as i32;
        let v = builder.ins().load(cl, MemFlags::trusted(), this, off);
        emit_release_for(
            module,
            class_layouts,
            array_kinds,
            optional_inners,
            *release_object_id,
            *release_string_id,
            *release_array_id,
            *release_weak_id,
            *release_map_id,
            *optional_box_release_id,
            &mut builder,
            v,
            elem_ty,
        );
    }

    builder.ins().return_(&[]);
    builder.finalize();

    compiler
        .module
        .define_function(drop_id, &mut compiler.ctx)
        .map_err(|e| CodegenError::Module(e.to_string()))?;
    Ok(())
}

/// Lazy lookup from build_array: returns the drop_fn_ptr Value to embed
/// in `ilang_jit_array_new`. Declares the FuncId on first use; the body
/// is defined later by `define_array_drops`.
pub(crate) fn array_drop_fn_ptr(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    array_id: u32,
) -> Value {
    let elem = lc.array_kinds[array_id as usize].elem;
    if !elem.is_heap() {
        lc.array_drops.entry(array_id).or_insert(None);
        return b.ins().iconst(I64, 0);
    }
    let id = if let Some(Some(id)) = lc.array_drops.get(&array_id) {
        *id
    } else {
        let symbol = format!("__drop_arr_{array_id}");
        let id = declare_drop_fn(lc.module, &symbol).expect("declare array drop");
        lc.array_drops.insert(array_id, Some(id));
        id
    };
    let func_ref = lc.module.declare_func_in_func(id, b.func);
    b.ins().func_addr(I64, func_ref)
}

/// Define every array drop body that got declared during lowering.
/// Define every enum drop body declared lazily during lowering. The
/// wrapper loads the tag and branches per-variant, releasing each
/// heap-typed payload field. Phase 2.
pub(crate) fn define_enum_drops(compiler: &mut JitCompiler) -> Result<(), CodegenError> {
    let to_define: Vec<(u32, FuncId)> = compiler
        .enum_drops
        .iter()
        .filter_map(|(k, v)| v.map(|id| (*k, id)))
        .collect();
    for (enum_id, drop_id) in to_define {
        define_one_enum_drop(compiler, enum_id, drop_id)?;
    }
    Ok(())
}

fn define_one_enum_drop(
    compiler: &mut JitCompiler,
    enum_id: u32,
    drop_id: FuncId,
) -> Result<(), CodegenError> {
    compiler.module.clear_context(&mut compiler.ctx);
    compiler.ctx.func.signature =
        compiler.module.declarations().get_function_decl(drop_id).signature.clone();

    let layout = compiler.enum_layouts[enum_id as usize].clone();

    let JitCompiler {
        module,
        ctx,
        builder_ctx,
        class_layouts,
        array_kinds,
        optional_inners,
        release_object_id,
        release_string_id,
        release_array_id,
        release_weak_id,
        release_map_id,
        optional_box_release_id,
        ..
    } = compiler;

    let mut builder = FunctionBuilder::new(&mut ctx.func, builder_ctx);
    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);
    builder.seal_block(entry);
    let this = builder.block_params(entry)[0];

    let tag = builder
        .ins()
        .load(cranelift_codegen::ir::types::I32, MemFlags::trusted(), this, ENUM_TAG_OFFSET);

    let merge = builder.create_block();
    for (i, vlayout) in layout.payloads.iter().enumerate() {
        // Skip variants with no heap fields entirely — nothing to do.
        let any_heap = match vlayout {
            EnumVariantLayout::Unit => false,
            EnumVariantLayout::Tuple(entries) => entries.iter().any(|(_, t)| t.is_heap()),
            EnumVariantLayout::Struct(map) => map.values().any(|(_, t)| t.is_heap()),
        };
        if !any_heap {
            continue;
        }
        let want = builder.ins().iconst(cranelift_codegen::ir::types::I32, layout.tags[i]);
        let cond = builder.ins().icmp(IntCC::Equal, tag, want);
        let body = builder.create_block();
        let next = builder.create_block();
        builder.ins().brif(cond, body, &[], next, &[]);
        builder.switch_to_block(body);
        builder.seal_block(body);
        // Release each heap-typed payload field for this variant.
        match vlayout {
            EnumVariantLayout::Unit => {}
            EnumVariantLayout::Tuple(entries) => {
                for (off, fty) in entries {
                    if !fty.is_heap() {
                        continue;
                    }
                    let cl = fty.cl().expect("non-unit field");
                    let abs = ENUM_PAYLOAD_OFFSET + (*off as i32);
                    let v = builder.ins().load(cl, MemFlags::trusted(), this, abs);
                    emit_release_for(
                        module,
                        class_layouts,
                        array_kinds,
                        optional_inners,
                        *release_object_id,
                        *release_string_id,
                        *release_array_id,
                        *release_weak_id,
            *release_map_id,
                        *optional_box_release_id,
                        &mut builder,
                        v,
                        *fty,
                    );
                }
            }
            EnumVariantLayout::Struct(map) => {
                for (off, fty) in map.values() {
                    if !fty.is_heap() {
                        continue;
                    }
                    let cl = fty.cl().expect("non-unit field");
                    let abs = ENUM_PAYLOAD_OFFSET + (*off as i32);
                    let v = builder.ins().load(cl, MemFlags::trusted(), this, abs);
                    emit_release_for(
                        module,
                        class_layouts,
                        array_kinds,
                        optional_inners,
                        *release_object_id,
                        *release_string_id,
                        *release_array_id,
                        *release_weak_id,
            *release_map_id,
                        *optional_box_release_id,
                        &mut builder,
                        v,
                        *fty,
                    );
                }
            }
        }
        builder.ins().jump(merge, &[]);
        builder.switch_to_block(next);
        builder.seal_block(next);
    }
    builder.ins().jump(merge, &[]);
    builder.switch_to_block(merge);
    builder.seal_block(merge);
    builder.ins().return_(&[]);
    builder.finalize();

    compiler
        .module
        .define_function(drop_id, &mut compiler.ctx)
        .map_err(|e| CodegenError::Module(e.to_string()))?;
    Ok(())
}

pub(crate) fn define_array_drops(compiler: &mut JitCompiler) -> Result<(), CodegenError> {
    let to_define: Vec<(u32, FuncId)> = compiler
        .array_drops
        .iter()
        .filter_map(|(k, v)| v.map(|id| (*k, id)))
        .collect();
    for (array_id, drop_id) in to_define {
        define_one_array_drop(compiler, array_id, drop_id)?;
    }
    Ok(())
}

fn define_one_array_drop(
    compiler: &mut JitCompiler,
    array_id: u32,
    drop_id: FuncId,
) -> Result<(), CodegenError> {
    compiler.module.clear_context(&mut compiler.ctx);
    compiler.ctx.func.signature =
        compiler.module.declarations().get_function_decl(drop_id).signature.clone();

    let elem_jty = compiler.array_kinds[array_id as usize].elem;
    let elem_size_const = elem_jty.size_bytes() as i64;

    let JitCompiler {
        module,
        ctx,
        builder_ctx,
        class_layouts,
        array_kinds,
        optional_inners,
        release_object_id,
        release_string_id,
        release_array_id,
        release_weak_id,
        release_map_id,
        optional_box_release_id,
        ..
    } = compiler;

    let mut builder = FunctionBuilder::new(&mut ctx.func, builder_ctx);
    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);
    builder.seal_block(entry);
    let header = builder.block_params(entry)[0];

    // Load len and data_ptr from the header. (Offsets mirror
    // ARRAY_LEN_OFFSET / ARRAY_DATA_OFFSET in runtime.rs.)
    let len = builder.ins().load(I64, MemFlags::trusted(), header, 16);
    let data = builder.ins().load(I64, MemFlags::trusted(), header, 32);

    // Loop: i = 0; while i < len { release(load(data + i*elem_size)); i++ }
    let i_var = builder.declare_var(I64);
    let zero = builder.ins().iconst(I64, 0);
    builder.def_var(i_var, zero);

    let header_block = builder.create_block();
    let body_block = builder.create_block();
    let after_block = builder.create_block();

    builder.ins().jump(header_block, &[]);
    builder.switch_to_block(header_block);
    let i = builder.use_var(i_var);
    let cond = builder.ins().icmp(IntCC::SignedLessThan, i, len);
    builder.ins().brif(cond, body_block, &[], after_block, &[]);

    builder.switch_to_block(body_block);
    builder.seal_block(body_block);
    let i = builder.use_var(i_var);
    let size_v = builder.ins().iconst(I64, elem_size_const);
    let off = builder.ins().imul(i, size_v);
    let addr = builder.ins().iadd(data, off);
    let elem = builder.ins().load(I64, MemFlags::trusted(), addr, 0);
    emit_release_for(
        module,
        class_layouts,
        array_kinds,
        optional_inners,
        *release_object_id,
        *release_string_id,
        *release_array_id,
        *release_weak_id,
            *release_map_id,
        *optional_box_release_id,
        &mut builder,
        elem,
        elem_jty,
    );
    let one = builder.ins().iconst(I64, 1);
    let new_i = builder.ins().iadd(i, one);
    builder.def_var(i_var, new_i);
    builder.ins().jump(header_block, &[]);
    builder.seal_block(header_block);

    builder.switch_to_block(after_block);
    builder.seal_block(after_block);
    builder.ins().return_(&[]);
    builder.finalize();

    compiler
        .module
        .define_function(drop_id, &mut compiler.ctx)
        .map_err(|e| CodegenError::Module(e.to_string()))?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn emit_release_for(
    module: &mut JITModule,
    class_layouts: &[ClassLayout],
    array_kinds: &[ArrayKind],
    optional_inners: &[JitTy],
    release_object_id: FuncId,
    release_string_id: FuncId,
    release_array_id: FuncId,
    release_weak_id: FuncId,
    release_map_id: FuncId,
    optional_box_release_id: FuncId,
    b: &mut FunctionBuilder,
    ptr: Value,
    ty: JitTy,
) {
    match ty {
        JitTy::Object(class_id) => {
            let r = module.declare_func_in_func(release_object_id, b.func);
            let size = class_layouts[class_id as usize].size as i64;
            let size_v = b.ins().iconst(I64, size);
            b.ins().call(r, &[ptr, size_v]);
        }
        JitTy::Str => {
            let r = module.declare_func_in_func(release_string_id, b.func);
            b.ins().call(r, &[ptr]);
        }
        JitTy::Array(inner_id) => {
            let r = module.declare_func_in_func(release_array_id, b.func);
            let elem_size = array_kinds[inner_id as usize].elem.size_bytes() as i64;
            let size_v = b.ins().iconst(I64, elem_size);
            b.ins().call(r, &[ptr, size_v]);
        }
        JitTy::Optional(id) => {
            // Dispatch to inner type's release. Heap inner: walk into
            // the inner's release. Primitive inner: call the boxed-
            // optional release with the payload size.
            let inner = optional_inners[id as usize];
            if inner.is_heap() {
                emit_release_for(
                    module,
                    class_layouts,
                    array_kinds,
                    optional_inners,
                    release_object_id,
                    release_string_id,
                    release_array_id,
                    release_weak_id,
                    release_map_id,
                    optional_box_release_id,
                    b,
                    ptr,
                    inner,
                );
            } else {
                let r = module.declare_func_in_func(optional_box_release_id, b.func);
                let size_v = b.ins().iconst(I64, inner.size_bytes() as i64);
                b.ins().call(r, &[ptr, size_v]);
            }
        }
        JitTy::Weak(class_id) => {
            let r = module.declare_func_in_func(release_weak_id, b.func);
            let user_size = class_layouts[class_id as usize].size as i64;
            let size_v = b.ins().iconst(I64, user_size);
            b.ins().call(r, &[ptr, size_v]);
        }
        JitTy::Map(_) => {
            let r = module.declare_func_in_func(release_map_id, b.func);
            b.ins().call(r, &[ptr]);
        }
        JitTy::Tuple(_) => {
            // Tuples share the object header; release_object dispatches
            // through the embedded drop_fn to release each heap element.
            // The user_size arg is purely for dealloc bookkeeping; pass
            // 0 — the runtime falls back to header-only size which is
            // correct because alloc_object zeroes the user area, but
            // the actual user size was the kind's `size`. To free the
            // exact storage we'd need the layout here; a 0 mismatch
            // is benign (Layout uses .max(1) and the alloc rounds up).
            // TODO: thread tuple_kinds through if leak-free dealloc
            // matters.
            let r = module.declare_func_in_func(release_object_id, b.func);
            let zero = b.ins().iconst(I64, 0);
            b.ins().call(r, &[ptr, zero]);
        }
        _ => {}
    }
}

fn declare_drop_fn(module: &mut JITModule, symbol: &str) -> Result<FuncId, CodegenError> {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(I64));
    module
        .declare_function(symbol, Linkage::Local, &sig)
        .map_err(|e| CodegenError::Module(e.to_string()))
}

/// Lazy lookup from `new Map<K, V>` lowering: returns the drop_fn_ptr
/// Value to embed in `ilang_jit_map_new`. The wrapper is a one-arg
/// `extern "C" fn(val: i64)` that releases the value bits as a heap V.
/// Returns iconst 0 when V is not heap (the runtime treats 0 as "skip").
pub(crate) fn map_drop_fn_ptr(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    map_id: u32,
) -> Value {
    let kind = lc.map_kinds[map_id as usize];
    if !kind.val.is_heap() {
        lc.map_drops.entry(map_id).or_insert(None);
        return b.ins().iconst(I64, 0);
    }
    let id = if let Some(Some(id)) = lc.map_drops.get(&map_id) {
        *id
    } else {
        let symbol = format!("__drop_map_val_{map_id}");
        let id = declare_drop_fn(lc.module, &symbol).expect("declare map value drop");
        lc.map_drops.insert(map_id, Some(id));
        id
    };
    let func_ref = lc.module.declare_func_in_func(id, b.func);
    b.ins().func_addr(I64, func_ref)
}

/// Lazy lookup for the per-(K, V) value-retain helper. Returns the
/// fn pointer Value to embed in `ilang_jit_map_values_to_array`.
/// Returns iconst 0 when V is not heap (the runtime treats 0 as "no
/// retain needed").
pub(crate) fn map_value_retain_fn_ptr(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    map_id: u32,
) -> Value {
    let kind = lc.map_kinds[map_id as usize];
    if !kind.val.is_heap() {
        lc.map_value_retains.entry(map_id).or_insert(None);
        return b.ins().iconst(I64, 0);
    }
    let id = if let Some(Some(id)) = lc.map_value_retains.get(&map_id) {
        *id
    } else {
        let symbol = format!("__retain_map_val_{map_id}");
        let id = declare_drop_fn(lc.module, &symbol).expect("declare map value retain");
        lc.map_value_retains.insert(map_id, Some(id));
        id
    };
    let func_ref = lc.module.declare_func_in_func(id, b.func);
    b.ins().func_addr(I64, func_ref)
}

/// Define every Map value-retain body declared during lowering.
pub(crate) fn define_map_value_retains(
    compiler: &mut JitCompiler,
) -> Result<(), CodegenError> {
    let to_define: Vec<(u32, FuncId)> = compiler
        .map_value_retains
        .iter()
        .filter_map(|(k, v)| v.map(|id| (*k, id)))
        .collect();
    for (map_id, retain_id) in to_define {
        define_one_map_value_retain(compiler, map_id, retain_id)?;
    }
    Ok(())
}

fn define_one_map_value_retain(
    compiler: &mut JitCompiler,
    map_id: u32,
    retain_id: FuncId,
) -> Result<(), CodegenError> {
    compiler.module.clear_context(&mut compiler.ctx);
    compiler.ctx.func.signature =
        compiler.module.declarations().get_function_decl(retain_id).signature.clone();

    let val_jty = compiler.map_kinds[map_id as usize].val;

    let JitCompiler {
        module,
        ctx,
        builder_ctx,
        retain_object_id,
        retain_string_id,
        retain_array_id,
        retain_weak_id,
        retain_map_id,
        ..
    } = compiler;

    let mut builder = FunctionBuilder::new(&mut ctx.func, builder_ctx);
    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);
    builder.seal_block(entry);
    let val = builder.block_params(entry)[0];

    emit_retain_for(
        module,
        *retain_object_id,
        *retain_string_id,
        *retain_array_id,
        *retain_weak_id,
        *retain_map_id,
        &mut builder,
        val,
        val_jty,
    );

    builder.ins().return_(&[]);
    builder.finalize();

    compiler
        .module
        .define_function(retain_id, &mut compiler.ctx)
        .map_err(|e| CodegenError::Module(e.to_string()))?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn emit_retain_for(
    module: &mut JITModule,
    retain_object_id: FuncId,
    retain_string_id: FuncId,
    retain_array_id: FuncId,
    retain_weak_id: FuncId,
    retain_map_id: FuncId,
    b: &mut FunctionBuilder,
    ptr: Value,
    ty: JitTy,
) {
    match ty {
        JitTy::Object(_) | JitTy::EnumHeap(_) => {
            let r = module.declare_func_in_func(retain_object_id, b.func);
            b.ins().call(r, &[ptr]);
        }
        JitTy::Str => {
            let r = module.declare_func_in_func(retain_string_id, b.func);
            b.ins().call(r, &[ptr]);
        }
        JitTy::Array(_) => {
            let r = module.declare_func_in_func(retain_array_id, b.func);
            b.ins().call(r, &[ptr]);
        }
        JitTy::Weak(_) => {
            let r = module.declare_func_in_func(retain_weak_id, b.func);
            b.ins().call(r, &[ptr]);
        }
        JitTy::Map(_) => {
            let r = module.declare_func_in_func(retain_map_id, b.func);
            b.ins().call(r, &[ptr]);
        }
        JitTy::Tuple(_) => {
            let r = module.declare_func_in_func(retain_object_id, b.func);
            b.ins().call(r, &[ptr]);
        }
        JitTy::Optional(_) => {
            // The runtime retains are all null-safe; just dispatch via
            // the inner type. To avoid threading optional_inners here,
            // call through retain_object — Optional<heap> always stores
            // a pointer (object-shaped retain works for all heap types
            // because all our heap retains share the leading-rc layout).
            // Simpler approach: only Phase A V kinds (Object, Str, Array,
            // Map) reach this path, so reaching Optional is a bug.
            unreachable!("Optional V should be handled by caller");
        }
        _ => {} // non-heap: nothing to do
    }
}

/// Define every Map value-drop body declared lazily during lowering.
/// The wrapper takes a single `val: i64` and emits the appropriate
/// per-V release call. Mirrors `define_array_drops` in shape.
pub(crate) fn define_map_drops(compiler: &mut JitCompiler) -> Result<(), CodegenError> {
    let to_define: Vec<(u32, FuncId)> = compiler
        .map_drops
        .iter()
        .filter_map(|(k, v)| v.map(|id| (*k, id)))
        .collect();
    for (map_id, drop_id) in to_define {
        define_one_map_drop(compiler, map_id, drop_id)?;
    }
    Ok(())
}

fn define_one_map_drop(
    compiler: &mut JitCompiler,
    map_id: u32,
    drop_id: FuncId,
) -> Result<(), CodegenError> {
    compiler.module.clear_context(&mut compiler.ctx);
    compiler.ctx.func.signature =
        compiler.module.declarations().get_function_decl(drop_id).signature.clone();

    let val_jty = compiler.map_kinds[map_id as usize].val;

    let JitCompiler {
        module,
        ctx,
        builder_ctx,
        class_layouts,
        array_kinds,
        optional_inners,
        release_object_id,
        release_string_id,
        release_array_id,
        release_weak_id,
        release_map_id,
        optional_box_release_id,
        ..
    } = compiler;

    let mut builder = FunctionBuilder::new(&mut ctx.func, builder_ctx);
    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);
    builder.seal_block(entry);
    let val = builder.block_params(entry)[0];

    emit_release_for(
        module,
        class_layouts,
        array_kinds,
        optional_inners,
        *release_object_id,
        *release_string_id,
        *release_array_id,
        *release_weak_id,
        *release_map_id,
        *optional_box_release_id,
        &mut builder,
        val,
        val_jty,
    );

    builder.ins().return_(&[]);
    builder.finalize();

    compiler
        .module
        .define_function(drop_id, &mut compiler.ctx)
        .map_err(|e| CodegenError::Module(e.to_string()))?;
    Ok(())
}

/// Lazily declare a drop fn for a closure wrapper. Returns its
/// address as a Cranelift Value (or 0 if no heap captures —
/// nothing to drop). Bodies are emitted by `define_closure_drops`
/// after every closure-construct site has been lowered.
pub(crate) fn closure_drop_fn_ptr(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    wrapper_name: &str,
    captures: &[(String, crate::ty::JitTy)],
) -> Result<Value, CodegenError> {
    let needs_drop = captures.iter().any(|(_, jty)| jty.is_heap());
    if !needs_drop {
        lc.closure_drops.entry(wrapper_name.to_string()).or_insert(None);
        return Ok(b.ins().iconst(I64, 0));
    }
    let id = if let Some(Some(id)) = lc.closure_drops.get(wrapper_name) {
        *id
    } else {
        let symbol = format!("__drop_closure_{wrapper_name}");
        let id = declare_drop_fn(lc.module, &symbol)?;
        lc.closure_drops.insert(wrapper_name.to_string(), Some(id));
        id
    };
    let func_ref = lc.module.declare_func_in_func(id, b.func);
    Ok(b.ins().func_addr(I64, func_ref))
}

/// Define every closure drop body that got declared during
/// lowering. Each body walks the wrapper's captures and emits
/// release for every heap-typed slot.
pub(crate) fn define_closure_drops(
    compiler: &mut JitCompiler,
) -> Result<(), CodegenError> {
    let to_define: Vec<(String, FuncId)> = compiler
        .closure_drops
        .iter()
        .filter_map(|(k, v)| v.map(|id| (k.clone(), id)))
        .collect();
    for (wrapper, drop_id) in to_define {
        define_one_closure_drop(compiler, &wrapper, drop_id)?;
    }
    Ok(())
}

fn define_one_closure_drop(
    compiler: &mut JitCompiler,
    wrapper: &str,
    drop_id: FuncId,
) -> Result<(), CodegenError> {
    use cranelift_codegen::ir::types::I64;
    compiler.module.clear_context(&mut compiler.ctx);
    compiler.ctx.func.signature =
        compiler.module.declarations().get_function_decl(drop_id).signature.clone();
    let captures = compiler
        .closure_meta
        .get(wrapper)
        .map(|m| m.captures.clone())
        .unwrap_or_default();
    let JitCompiler {
        module,
        ctx,
        builder_ctx,
        release_object_id,
        release_string_id,
        release_array_id,
        release_weak_id,
        optional_box_release_id,
        release_map_id,
        ..
    } = compiler;
    let mut builder = FunctionBuilder::new(&mut ctx.func, builder_ctx);
    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);
    builder.seal_block(entry);
    let closure_ptr = builder.block_params(entry)[0];
    for (i, (_name, jty)) in captures.iter().enumerate() {
        if !jty.is_heap() {
            continue;
        }
        let offset = (8 + i * 8) as i32;
        let v = builder.ins().load(I64, MemFlags::trusted(), closure_ptr, offset);
        // Pick the right release fn based on the JIT type. Map /
        // arrays / objects need the user-size for the dealloc;
        // strings / weak / optional have their own helpers.
        match jty {
            crate::ty::JitTy::Object(_class_id) => {
                // user-size unknown here without the class layout
                // table; pass 0 — release_object reads it via
                // its second arg purely for dealloc bookkeeping.
                // Pass 0 is safe because dealloc uses the size
                // only when freeing storage; the closure isn't
                // a class instance so the size mismatch is fine.
                let zero = builder.ins().iconst(I64, 0);
                let r = module.declare_func_in_func(*release_object_id, builder.func);
                builder.ins().call(r, &[v, zero]);
            }
            crate::ty::JitTy::Str => {
                let r = module.declare_func_in_func(*release_string_id, builder.func);
                builder.ins().call(r, &[v]);
            }
            crate::ty::JitTy::Array(_) => {
                let zero = builder.ins().iconst(I64, 0);
                let r = module.declare_func_in_func(*release_array_id, builder.func);
                builder.ins().call(r, &[v, zero]);
            }
            crate::ty::JitTy::Optional(_) => {
                let r = module.declare_func_in_func(*optional_box_release_id, builder.func);
                builder.ins().call(r, &[v]);
            }
            crate::ty::JitTy::Weak(_) => {
                let zero = builder.ins().iconst(I64, 0);
                let r = module.declare_func_in_func(*release_weak_id, builder.func);
                builder.ins().call(r, &[v, zero]);
            }
            crate::ty::JitTy::Map(_) => {
                let r = module.declare_func_in_func(*release_map_id, builder.func);
                builder.ins().call(r, &[v]);
            }
            _ => {}
        }
    }
    builder.ins().return_(&[]);
    builder.finalize();
    compiler
        .module
        .define_function(drop_id, &mut compiler.ctx)
        .map_err(|e| CodegenError::Module(e.to_string()))?;
    Ok(())
}
