//! Ahead-of-time object file emission.
//!
//! Delegates the per-function lowering to `compile::lower_program_into`
//! so the JIT and AOT backends share one implementation. The AOT
//! side is responsible for the surrounding pieces only:
//!
//! - constructing an `ObjectModule` (PIC enabled) instead of a JIT,
//! - assigning global class / enum ids (the JIT does this too),
//! - emitting a C-ABI `main` wrapper that calls the user entry and
//!   folds its `i64` return into a process exit code,
//! - turning the finished `ObjectProduct` into bytes for the CLI.
//!
//! Programs that use heap types, virtual dispatch, or `@extern("lib")`
//! still lower to references against unresolved runtime symbols —
//! the system linker reports them as undefined symbols, which is
//! clearer than us trying to mirror the JIT's host-table state.

use cranelift::prelude::*;
use cranelift_codegen::ir::{AbiParam, Function as ClifFunc, InstBuilder, UserFuncName};
use cranelift_codegen::settings;
use cranelift_frontend::{FunctionBuilder as ClifFnBuilder, FunctionBuilderContext};
use cranelift_module::{Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};

use ilang_mir::{FuncId, MirTy, Program};

use crate::compile::{
    alloc_global_class_id, alloc_global_enum_id, lower_program_into, CompileError,
};
use crate::ty::mir_to_clif;

#[derive(Debug, thiserror::Error)]
pub enum AotError {
    #[error("AOT does not yet support: {0}")]
    Unsupported(String),
    #[error("{0}")]
    Other(String),
    #[error(transparent)]
    Module(#[from] cranelift_module::ModuleError),
    #[error("{0}")]
    Compile(String),
}

impl From<CompileError> for AotError {
    fn from(e: CompileError) -> Self {
        AotError::Compile(e.to_string())
    }
}

/// Compile `prog` to a Mach-O / ELF / COFF object file (depending on
/// host) and return the raw bytes. The emitted module exports the
/// user entry under `__ilang_main` and a C ABI `main` wrapper that
/// calls it and truncates the result to the process exit code (i32).
pub fn compile_program_to_object(prog: &Program) -> Result<Vec<u8>, AotError> {
    let entry = &prog.functions[prog.entry.0 as usize];
    validate_subset(prog, entry)?;

    let entry_clif_ret = if matches!(entry.ret, MirTy::Unit) {
        None
    } else {
        Some(mir_to_clif(&entry.ret).ok_or_else(|| {
            AotError::Unsupported(format!("entry return type {:?}", entry.ret))
        })?)
    };

    let isa_builder = cranelift_native::builder()
        .map_err(|e| AotError::Other(format!("cranelift_native: {e}")))?;
    let mut flag_builder = settings::builder();
    // ObjectModule requires PIC; the JIT path doesn't.
    flag_builder
        .set("is_pic", "true")
        .map_err(|e| AotError::Other(format!("set is_pic: {e}")))?;
    let isa = isa_builder
        .finish(settings::Flags::new(flag_builder))
        .map_err(|e| AotError::Other(format!("isa: {e}")))?;

    let builder = ObjectBuilder::new(
        isa,
        b"ilang_aot".to_vec(),
        cranelift_module::default_libcall_names(),
    )
    .map_err(|e| AotError::Other(format!("ObjectBuilder: {e}")))?;
    let mut module = ObjectModule::new(builder);

    // Allocate global class / enum ids, matching what compile.rs does
    // for the JIT. The lowering reads these to embed stable ids into
    // class headers and enum discriminants.
    let class_global: Vec<u32> = (0..prog.classes.len())
        .map(|_| alloc_global_class_id())
        .collect();
    let enum_global: Vec<u32> = (0..prog.enums.len())
        .map(|_| alloc_global_enum_id())
        .collect();

    // Shared lowering pass: declares every user fn (and the full
    // runtime-symbol import set), pre-defines string-literal data,
    // and lowers every fn body to clif IR. Returns the FuncId map for
    // the entry-wrapping step below.
    let outputs = lower_program_into(
        &mut module,
        prog,
        &[],
        &class_global,
        &enum_global,
    )?;

    let entry_id = *outputs.fn_ids.get(&prog.entry).ok_or_else(|| {
        AotError::Other("entry fn not registered after lowering".into())
    })?;
    // Re-tag the entry as Export so the linker can resolve `main` ->
    // the user entry. `lower_program_into` declared it Local; bumping
    // linkage via re-declare with the same name is idempotent at the
    // module-declarations level.
    {
        let func = &prog.functions[prog.entry.0 as usize];
        let symbol_name: &str = if let Some(c) = func.c_symbol {
            c.as_str()
        } else {
            func.name.as_str()
        };
        // Reuse the existing signature stored on the fn id.
        let entry_sig = module
            .declarations()
            .get_function_decl(entry_id)
            .signature
            .clone();
        module.declare_function(symbol_name, Linkage::Export, &entry_sig)?;
    }

    let mut ctx = module.make_context();
    let mut fb_ctx = FunctionBuilderContext::new();

    // Emit `__ilang_aot_init()` — runs at process startup (via `main`
    // below) and populates the runtime dispatch tables that the JIT
    // backfills after `finalize_definitions`. AOT can't do the same
    // at codegen time because the addresses don't exist until the OS
    // dynamic linker maps the executable, so we generate IR that calls
    // `__register_vtable_entry` / `__register_drop` with `func_addr`
    // values resolved at load time.
    let aot_init_id = emit_aot_init(
        &mut module,
        &mut ctx,
        &mut fb_ctx,
        prog,
        &class_global,
        &outputs.fn_ids,
    )?;

    // Emit the C ABI `main` wrapper. `Linkage::Export` exposes it so
    // the platform's startup code resolves `_main` / `main` here.
    let mut main_sig = module.make_signature();
    main_sig.returns.push(AbiParam::new(types::I32));
    let main_id = module.declare_function("main", Linkage::Export, &main_sig)?;
    ctx.func = ClifFunc::with_name_signature(
        UserFuncName::user(0, main_id.as_u32()),
        main_sig.clone(),
    );
    {
        let mut fb = ClifFnBuilder::new(&mut ctx.func, &mut fb_ctx);
        let block = fb.create_block();
        fb.switch_to_block(block);
        fb.seal_block(block);
        // Call the AOT init first so vtable / drop lookups succeed
        // by the time `__ilang_main` runs.
        let init_ref = module.declare_func_in_func(aot_init_id, fb.func);
        fb.ins().call(init_ref, &[]);
        let entry_ref = module.declare_func_in_func(entry_id, fb.func);
        // The shared lowering signs every user fn with a trailing
        // hidden `env: i64` slot (so closures and free fns share one
        // ABI). The entry isn't a closure, so pass null.
        let env_null = fb.ins().iconst(types::I64, 0);
        let call = fb.ins().call(entry_ref, &[env_null]);
        let ret32 = match entry_clif_ret {
            Some(_) => {
                let v = fb.inst_results(call)[0];
                coerce_to_i32(&mut fb, v, &entry.ret)
            }
            None => fb.ins().iconst(types::I32, 0),
        };
        fb.ins().return_(&[ret32]);
        fb.finalize();
    }
    module
        .define_function(main_id, &mut ctx)
        .map_err(|e| AotError::Other(format!("define_function main: {e:?}")))?;

    let product = module.finish();
    product
        .emit()
        .map_err(|e| AotError::Other(format!("emit object: {e}")))
}

/// Emit a private `__ilang_aot_init()` function that fills the
/// runtime's vtable / drop tables from the program's class metadata
/// at process startup. The C `main` wrapper calls this before
/// `__ilang_main`.
fn emit_aot_init(
    module: &mut ObjectModule,
    ctx: &mut cranelift_codegen::Context,
    fb_ctx: &mut FunctionBuilderContext,
    prog: &Program,
    class_global: &[u32],
    fn_ids: &std::collections::HashMap<FuncId, cranelift_module::FuncId>,
) -> Result<cranelift_module::FuncId, AotError> {
    // Imports.
    let reg_vtable = {
        let mut s = module.make_signature();
        s.params.push(AbiParam::new(types::I64));
        s.params.push(AbiParam::new(types::I64));
        s.params.push(AbiParam::new(types::I64));
        module.declare_function("__register_vtable_entry", Linkage::Import, &s)?
    };
    let reg_drop = {
        let mut s = module.make_signature();
        s.params.push(AbiParam::new(types::I64));
        s.params.push(AbiParam::new(types::I64));
        module.declare_function("__register_drop", Linkage::Import, &s)?
    };
    let reg_class_size = {
        let mut s = module.make_signature();
        s.params.push(AbiParam::new(types::I64));
        s.params.push(AbiParam::new(types::I64));
        module.declare_function("__register_class_size", Linkage::Import, &s)?
    };

    let init_sig = module.make_signature();
    let init_id =
        module.declare_function("__ilang_aot_init", Linkage::Local, &init_sig)?;
    ctx.func = ClifFunc::with_name_signature(
        UserFuncName::user(0, init_id.as_u32()),
        init_sig,
    );
    {
        let mut fb = ClifFnBuilder::new(&mut ctx.func, fb_ctx);
        let block = fb.create_block();
        fb.switch_to_block(block);
        fb.seal_block(block);

        let reg_vtable_ref = module.declare_func_in_func(reg_vtable, fb.func);
        let reg_drop_ref = module.declare_func_in_func(reg_drop, fb.func);
        let reg_class_size_ref = module.declare_func_in_func(reg_class_size, fb.func);

        for class in &prog.classes {
            let global_cid = class_global[class.id.0 as usize] as i64;
            // Register the byte size of this class's heap allocation
            // so `__release_object` can reclaim the buffer at rc=0.
            // Skip CRepr / packed / union classes — their lifetime is
            // already tracked at the codegen level via direct
            // `__mir_free(ptr, c_size)` emits.
            let skip_free = matches!(
                class.repr,
                ilang_mir::ClassRepr::CRepr
                    | ilang_mir::ClassRepr::CPacked
                    | ilang_mir::ClassRepr::CUnion
            );
            if !skip_free {
                // 16-byte header (class_id + rc) + 8 bytes per field.
                let size = 16 + (class.fields.len() as i64) * 8;
                let cid_v = fb.ins().iconst(types::I64, global_cid);
                let size_v = fb.ins().iconst(types::I64, size);
                fb.ins().call(reg_class_size_ref, &[cid_v, size_v]);
            }
            // Vtable entries: every method with a slot maps to its fn
            // address at the global (class_id, slot) key.
            for m in &class.methods {
                if let Some(slot) = m.slot {
                    if let Some(&cl_id) = fn_ids.get(&m.func) {
                        let fr = module.declare_func_in_func(cl_id, fb.func);
                        let addr = fb.ins().func_addr(types::I64, fr);
                        let cid_v = fb.ins().iconst(types::I64, global_cid);
                        let slot_v = fb.ins().iconst(types::I64, slot.0 as i64);
                        fb.ins().call(reg_vtable_ref, &[cid_v, slot_v, addr]);
                    }
                }
            }
            // Drop entry, if the class has a deinit lowered to a fn.
            if class.drop_fn.0 != u32::MAX {
                if let Some(&cl_id) = fn_ids.get(&class.drop_fn) {
                    let fr = module.declare_func_in_func(cl_id, fb.func);
                    let addr = fb.ins().func_addr(types::I64, fr);
                    let cid_v = fb.ins().iconst(types::I64, global_cid);
                    fb.ins().call(reg_drop_ref, &[cid_v, addr]);
                }
            }
        }

        fb.ins().return_(&[]);
        fb.finalize();
    }
    module
        .define_function(init_id, ctx)
        .map_err(|e| AotError::Other(format!("define_function __ilang_aot_init: {e:?}")))?;
    module.clear_context(ctx);
    Ok(init_id)
}

/// Fold the entry's return value into a process exit code (i32). Bool
/// and narrow ints widen / narrow appropriately; floats convert with
/// saturation; unsupported types fall through as zero.
fn coerce_to_i32(fb: &mut ClifFnBuilder, v: Value, ty: &MirTy) -> Value {
    let cur = fb.func.dfg.value_type(v);
    if cur == types::I32 {
        return v;
    }
    if cur.is_int() {
        let cur_bits = cur.bits();
        let dst_bits = types::I32.bits();
        if cur_bits < dst_bits {
            return if matches!(
                ty,
                MirTy::I8 | MirTy::I16 | MirTy::I32 | MirTy::I64
            ) {
                fb.ins().sextend(types::I32, v)
            } else {
                fb.ins().uextend(types::I32, v)
            };
        }
        return fb.ins().ireduce(types::I32, v);
    }
    if cur == types::F64 || cur == types::F32 {
        return fb.ins().fcvt_to_sint_sat(types::I32, v);
    }
    fb.ins().iconst(types::I32, 0)
}

/// Reject programs that pull in runtime tables the AOT path does not
/// populate yet (classes via vtable, enums with payload, etc.). These
/// would silently dispatch through empty `VTABLE` / `DROP_TABLE`
/// statics at runtime — better to fail at build time.
fn validate_subset(
    prog: &Program,
    entry: &ilang_mir::Function,
) -> Result<(), AotError> {
    // Classes lower through the same NewObject / LoadField paths the
    // JIT uses. Programs that rely on `__virt_dispatch` / `__drop_dispatch`
    // or other runtime-dispatch tables fail at the linker — the runtime
    // crate ships no-op `__retain_object` / `__release_object` until the
    // table-population init-emit lands.
    if !prog.statics.is_empty() {
        return Err(AotError::Unsupported(
            "static slots — not yet wired into AOT".into(),
        ));
    }
    if !entry.params.is_empty() {
        return Err(AotError::Unsupported(
            "entry function with parameters (expected `() -> T`)".into(),
        ));
    }
    if entry.closure_env.is_some() {
        return Err(AotError::Unsupported(
            "closure entry function".into(),
        ));
    }
    // Allow user-defined functions and the entire shared MIR lowering
    // surface — the linker will surface any runtime symbols we don't
    // yet ship in `ilang-runtime`.
    let _ = FuncId(0);
    Ok(())
}
