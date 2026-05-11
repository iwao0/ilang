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
use cranelift_module::{DataDescription, DataId, Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};

use std::sync::atomic::{AtomicU64, Ordering};

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
        &enum_global,
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

static AOT_STR_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Emit a `[ i64 length | bytes | \0 ]` data symbol so init code can
/// hand its body pointer to runtime `__register_*_print_*` calls.
/// Names are uniqued via a process-wide counter so parallel compiles
/// (incremental rebuilds, parallel tests) don't trample each other.
fn declare_ilang_string_data(
    module: &mut ObjectModule,
    text: &str,
) -> Result<DataId, AotError> {
    let n = AOT_STR_COUNTER.fetch_add(1, Ordering::Relaxed);
    let sym = format!("__aot_str_{n}");
    let body = text.as_bytes();
    let mut bytes: Vec<u8> = Vec::with_capacity(8 + body.len() + 1);
    bytes.extend_from_slice(&(body.len() as i64).to_le_bytes());
    bytes.extend_from_slice(body);
    bytes.push(0);
    let mut desc = DataDescription::new();
    desc.set_align(8);
    desc.define(bytes.into_boxed_slice());
    let did = module.declare_data(&sym, Linkage::Local, false, false)?;
    module.define_data(did, &desc).map_err(AotError::Module)?;
    Ok(did)
}

/// Emit IR to load the body-pointer of the data symbol (address + 8
/// bytes past the length prefix). Mirrors the codegen's string
/// literal pointer convention.
fn ilang_string_body(
    module: &mut ObjectModule,
    fb: &mut ClifFnBuilder,
    did: DataId,
) -> Value {
    let gv = module.declare_data_in_func(did, fb.func);
    let base = fb.ins().symbol_value(types::I64, gv);
    let off8 = fb.ins().iconst(types::I64, 8);
    fb.ins().iadd(base, off8)
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
    enum_global: &[u32],
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
    let reg_object_field = {
        let mut s = module.make_signature();
        s.params.push(AbiParam::new(types::I64));
        s.params.push(AbiParam::new(types::I64));
        s.params.push(AbiParam::new(types::I64));
        module.declare_function("__register_object_field", Linkage::Import, &s)?
    };
    let reg_closure_capture = {
        let mut s = module.make_signature();
        s.params.push(AbiParam::new(types::I64));
        s.params.push(AbiParam::new(types::I64));
        s.params.push(AbiParam::new(types::I64));
        module.declare_function("__register_closure_capture", Linkage::Import, &s)?
    };
    let reg_closure_size = {
        let mut s = module.make_signature();
        s.params.push(AbiParam::new(types::I64));
        s.params.push(AbiParam::new(types::I64));
        module.declare_function("__register_closure_size", Linkage::Import, &s)?
    };
    let reg_enum_payload_kind = {
        let mut s = module.make_signature();
        s.params.push(AbiParam::new(types::I64));
        s.params.push(AbiParam::new(types::I64));
        s.params.push(AbiParam::new(types::I64));
        s.params.push(AbiParam::new(types::I64));
        module.declare_function("__register_enum_payload_kind", Linkage::Import, &s)?
    };
    let reg_class_print_name = {
        let mut s = module.make_signature();
        s.params.push(AbiParam::new(types::I64));
        s.params.push(AbiParam::new(types::I64));
        module.declare_function("__register_class_print_name", Linkage::Import, &s)?
    };
    let reg_class_print_field = {
        let mut s = module.make_signature();
        s.params.push(AbiParam::new(types::I64));
        s.params.push(AbiParam::new(types::I64));
        s.params.push(AbiParam::new(types::I64));
        s.params.push(AbiParam::new(types::I64));
        module.declare_function("__register_class_print_field", Linkage::Import, &s)?
    };
    let reg_enum_print_name = {
        let mut s = module.make_signature();
        s.params.push(AbiParam::new(types::I64));
        s.params.push(AbiParam::new(types::I64));
        module.declare_function("__register_enum_print_name", Linkage::Import, &s)?
    };
    let reg_enum_print_variant_name = {
        let mut s = module.make_signature();
        s.params.push(AbiParam::new(types::I64));
        s.params.push(AbiParam::new(types::I64));
        s.params.push(AbiParam::new(types::I64));
        module.declare_function(
            "__register_enum_print_variant_name",
            Linkage::Import,
            &s,
        )?
    };
    let reg_enum_print_variant_payload_pk = {
        let mut s = module.make_signature();
        s.params.push(AbiParam::new(types::I64));
        s.params.push(AbiParam::new(types::I64));
        s.params.push(AbiParam::new(types::I64));
        s.params.push(AbiParam::new(types::I64));
        module.declare_function(
            "__register_enum_print_variant_payload_pk",
            Linkage::Import,
            &s,
        )?
    };

    // Pre-allocate data symbols for every class / field / enum /
    // variant name so init-body IR can hand body pointers to the
    // runtime registrations. Done before the function-body block so
    // module-level declarations don't fight the FunctionBuilder for
    // module access.
    let mut class_name_data: Vec<DataId> = Vec::with_capacity(prog.classes.len());
    let mut class_field_name_data: Vec<Vec<DataId>> =
        Vec::with_capacity(prog.classes.len());
    for class in &prog.classes {
        class_name_data.push(declare_ilang_string_data(module, class.name.as_str())?);
        let mut per_class = Vec::with_capacity(class.fields.len());
        for f in &class.fields {
            per_class.push(declare_ilang_string_data(module, f.name.as_str())?);
        }
        class_field_name_data.push(per_class);
    }
    let mut enum_name_data: Vec<DataId> = Vec::with_capacity(prog.enums.len());
    let mut enum_variant_name_data: Vec<Vec<DataId>> =
        Vec::with_capacity(prog.enums.len());
    for e in &prog.enums {
        enum_name_data.push(declare_ilang_string_data(module, e.name.as_str())?);
        let mut per_enum = Vec::with_capacity(e.variants.len());
        for v in &e.variants {
            per_enum.push(declare_ilang_string_data(module, v.name.as_str())?);
        }
        enum_variant_name_data.push(per_enum);
    }

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
        let reg_object_field_ref =
            module.declare_func_in_func(reg_object_field, fb.func);
        let reg_closure_capture_ref =
            module.declare_func_in_func(reg_closure_capture, fb.func);
        let reg_closure_size_ref =
            module.declare_func_in_func(reg_closure_size, fb.func);
        let reg_enum_payload_kind_ref =
            module.declare_func_in_func(reg_enum_payload_kind, fb.func);
        let reg_class_print_name_ref =
            module.declare_func_in_func(reg_class_print_name, fb.func);
        let reg_class_print_field_ref =
            module.declare_func_in_func(reg_class_print_field, fb.func);
        let reg_enum_print_name_ref =
            module.declare_func_in_func(reg_enum_print_name, fb.func);
        let reg_enum_print_variant_name_ref =
            module.declare_func_in_func(reg_enum_print_variant_name, fb.func);
        let reg_enum_print_variant_payload_pk_ref =
            module.declare_func_in_func(reg_enum_print_variant_payload_pk, fb.func);

        for (cls_idx, class) in prog.classes.iter().enumerate() {
            let global_cid = class_global[class.id.0 as usize] as i64;
            // Print info: class name + per-field (name, PK_*).
            let cid_v = fb.ins().iconst(types::I64, global_cid);
            let name_did = class_name_data[cls_idx];
            let name_body = ilang_string_body(module, &mut fb, name_did);
            fb.ins().call(reg_class_print_name_ref, &[cid_v, name_body]);
            for (fi, f) in class.fields.iter().enumerate() {
                let pk = print_kind_id_for_ty(&f.ty);
                let fname_did = class_field_name_data[cls_idx][fi];
                let fname_body = ilang_string_body(module, &mut fb, fname_did);
                let idx_v = fb.ins().iconst(types::I64, fi as i64);
                let pk_v = fb.ins().iconst(types::I64, pk);
                fb.ins().call(
                    reg_class_print_field_ref,
                    &[cid_v, idx_v, fname_body, pk_v],
                );
            }
            // Heap-typed fields go into the runtime's
            // `OBJECT_FIELD_TABLE` so `__release_object_fields`
            // cascades through them at rc=0. Each entry is the byte
            // offset within the cell (header 16 B + 8 B * idx) plus
            // the `KIND_*` tag.
            for (i, f) in class.fields.iter().enumerate() {
                let tag = field_kind_tag(&f.ty);
                if tag == 0 {
                    continue; // KIND_NONE — primitive, no cascade.
                }
                let off = 16 + (i as i64) * 8;
                let cid_v = fb.ins().iconst(types::I64, global_cid);
                let off_v = fb.ins().iconst(types::I64, off);
                let tag_v = fb.ins().iconst(types::I64, tag);
                fb.ins().call(reg_object_field_ref, &[cid_v, off_v, tag_v]);
            }
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

        // Closure capture / size tables — keyed by fn_addr at runtime
        // so `__release_closure` can walk the cell's heap-shaped
        // captures and free the right block size.
        for (idx, func) in prog.functions.iter().enumerate() {
            let env = match &func.closure_env {
                Some(e) => e,
                None => continue,
            };
            let mid = FuncId(idx as u32);
            let cl_id = match fn_ids.get(&mid) {
                Some(c) => *c,
                None => continue,
            };
            let fr = module.declare_func_in_func(cl_id, fb.func);
            let fn_addr = fb.ins().func_addr(types::I64, fr);
            for (i, cap) in env.captures.iter().enumerate() {
                if cap.is_cell {
                    continue; // cells stay outside the registry
                }
                let tag = field_kind_tag(&cap.ty);
                if tag == 0 {
                    continue;
                }
                let off = 16 + (i as i64) * 8;
                let off_v = fb.ins().iconst(types::I64, off);
                let tag_v = fb.ins().iconst(types::I64, tag);
                fb.ins().call(reg_closure_capture_ref, &[fn_addr, off_v, tag_v]);
            }
            let total_size = (2 + env.captures.len() as i64) * 8;
            let size_v = fb.ins().iconst(types::I64, total_size);
            fb.ins().call(reg_closure_size_ref, &[fn_addr, size_v]);
        }

        // Enum payload kinds (cascade) + enum print info (name +
        // per-variant name + per-payload PK_*).
        for (idx, e) in prog.enums.iter().enumerate() {
            let global_id = enum_global[idx] as i64;
            let eid_v = fb.ins().iconst(types::I64, global_id);
            // Print: enum name.
            let ename_body = ilang_string_body(module, &mut fb, enum_name_data[idx]);
            fb.ins().call(reg_enum_print_name_ref, &[eid_v, ename_body]);
            for (vi, v) in e.variants.iter().enumerate() {
                let disc_v = fb.ins().iconst(types::I64, v.discriminant);
                // Print: variant name.
                let vname_body = ilang_string_body(
                    module,
                    &mut fb,
                    enum_variant_name_data[idx][vi],
                );
                fb.ins().call(
                    reg_enum_print_variant_name_ref,
                    &[eid_v, disc_v, vname_body],
                );
                let payload_tys: Vec<&MirTy> = match &v.payload {
                    ilang_mir::VariantPayload::Unit => Vec::new(),
                    ilang_mir::VariantPayload::Tuple(tys) => tys.iter().collect(),
                    ilang_mir::VariantPayload::Struct(fs) => {
                        fs.iter().map(|(_, t)| t).collect()
                    }
                };
                for (i, ty) in payload_tys.iter().enumerate() {
                    // Cascade tag.
                    let tag = field_kind_tag(ty);
                    if tag != 0 {
                        let slot_v = fb.ins().iconst(types::I64, i as i64);
                        let tag_v = fb.ins().iconst(types::I64, tag);
                        fb.ins().call(
                            reg_enum_payload_kind_ref,
                            &[eid_v, disc_v, slot_v, tag_v],
                        );
                    }
                    // Print tag (PK_*).
                    let pk = print_kind_id_for_ty(ty);
                    let slot_v = fb.ins().iconst(types::I64, i as i64);
                    let pk_v = fb.ins().iconst(types::I64, pk);
                    fb.ins().call(
                        reg_enum_print_variant_payload_pk_ref,
                        &[eid_v, disc_v, slot_v, pk_v],
                    );
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

/// Map a MIR type to the runtime's `PK_*` print tag. Mirrors
/// `compile::print_kind_id`.
fn print_kind_id_for_ty(ty: &MirTy) -> i64 {
    match ty {
        MirTy::I64 | MirTy::Size | MirTy::SSize => 0,  // PK_I64_SIG
        MirTy::U64 => 1,                               // PK_I64_UNS
        MirTy::I32 => 2,                               // PK_I32_SIG
        MirTy::U32 => 3,                               // PK_I32_UNS
        MirTy::I16 => 4,                               // PK_I16_SIG
        MirTy::U16 => 5,                               // PK_I16_UNS
        MirTy::I8 | MirTy::CChar => 6,                 // PK_I8_SIG
        MirTy::U8 => 7,                                // PK_I8_UNS
        MirTy::Bool => 8,                              // PK_BOOL
        MirTy::F64 => 9,                               // PK_F64
        MirTy::F32 => 10,                              // PK_F32
        MirTy::Str => 11,                              // PK_STR
        MirTy::Object(_) => 12,                        // PK_OBJECT
        MirTy::Array { elem, .. } if matches!(**elem, MirTy::I64) => 100, // PK_ARRAY_I64_SIG
        _ => -1,                                       // PK_OTHER
    }
}

/// Map a MIR field type to the runtime's `KIND_*` cascade tag.
/// Returns 0 (`KIND_NONE`) for primitives that need no cascade.
fn field_kind_tag(ty: &MirTy) -> i64 {
    match ty {
        MirTy::Object(_) => 1,    // KIND_OBJECT
        MirTy::Array { .. } => 2, // KIND_ARRAY
        MirTy::Optional(_) => 3,  // KIND_OPTIONAL
        MirTy::Tuple(_) => 4,     // KIND_TUPLE
        MirTy::Map { .. } => 5,   // KIND_MAP
        MirTy::Fn(_) => 6,        // KIND_CLOSURE
        MirTy::Str => 7,          // KIND_STR
        MirTy::Enum(_) => 8,      // KIND_ENUM
        _ => 0,                   // KIND_NONE
    }
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
