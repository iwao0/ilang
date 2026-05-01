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
use crate::ty::{ArrayKind, ClassLayout, JitTy};

/// True when a class needs a drop wrapper at all (heap field or deinit).
fn class_needs_drop(compiler: &JitCompiler, class_id: u32) -> bool {
    let layout = &compiler.class_layouts[class_id as usize];
    let has_heap_field = layout
        .fields
        .values()
        .any(|(_, fty)| matches!(fty, JitTy::Object(_) | JitTy::Str | JitTy::Array(_)));
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
        .filter(|(_, fty)| matches!(fty, JitTy::Object(_) | JitTy::Str | JitTy::Array(_)))
        .copied()
        .collect();

    let JitCompiler {
        module,
        ctx,
        builder_ctx,
        class_layouts,
        array_kinds,
        release_object_id,
        release_string_id,
        release_array_id,
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
            *release_object_id,
            *release_string_id,
            *release_array_id,
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

/// Lazy lookup from build_array: returns the drop_fn_ptr Value to embed
/// in `ilang_jit_array_new`. Declares the FuncId on first use; the body
/// is defined later by `define_array_drops`.
pub(crate) fn array_drop_fn_ptr(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    array_id: u32,
) -> Value {
    let elem = lc.array_kinds[array_id as usize].elem;
    if !matches!(elem, JitTy::Object(_) | JitTy::Str | JitTy::Array(_)) {
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
        release_object_id,
        release_string_id,
        release_array_id,
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
    let i_var = Variable::new(0);
    builder.declare_var(i_var, I64);
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
        *release_object_id,
        *release_string_id,
        *release_array_id,
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
    release_object_id: FuncId,
    release_string_id: FuncId,
    release_array_id: FuncId,
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
