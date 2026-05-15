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

mod helpers;

use helpers::{coerce_to_i32, field_kind_tag, print_kind_id_for_ty, validate_subset};

use crate::compile::{
    alloc_global_class_id, alloc_global_enum_id, lower_program_into_with_missing,
    CompileError,
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

#[cfg(not(windows))]
unsafe extern "C" {
    fn dlopen(path: *const u8, flags: i32) -> *mut u8;
}
#[cfg(not(windows))]
const RTLD_LAZY: i32 = 1;

#[cfg(windows)]
unsafe extern "system" {
    fn LoadLibraryA(lpFileName: *const u8) -> *mut u8;
}

fn lib_loadable(name: &str) -> bool {
    let try_one = |n: &str| -> bool {
        let mut nul = n.as_bytes().to_vec();
        nul.push(0);
        #[cfg(not(windows))]
        let h = unsafe { dlopen(nul.as_ptr(), RTLD_LAZY) };
        #[cfg(windows)]
        let h = unsafe { LoadLibraryA(nul.as_ptr()) };
        !h.is_null()
    };
    if try_one(name) {
        return true;
    }
    if !name.contains('.') && !name.contains('/') {
        let candidates: Vec<String> = if cfg!(target_os = "macos") {
            vec![
                format!("lib{name}.dylib"),
                format!("{name}.dylib"),
                format!("/opt/homebrew/lib/lib{name}.dylib"),
                format!("/opt/homebrew/lib/{name}.dylib"),
                format!("/usr/local/lib/lib{name}.dylib"),
                format!("/usr/local/lib/{name}.dylib"),
            ]
        } else if cfg!(target_os = "windows") {
            vec![format!("{name}.dll"), format!("lib{name}.dll")]
        } else {
            let mut out = vec![format!("lib{name}.so")];
            for n in [6, 5, 4, 3, 2, 1, 0] {
                out.push(format!("lib{name}.so.{n}"));
            }
            out
        };
        for cand in candidates {
            if try_one(&cand) {
                return true;
            }
        }
    }
    false
}

/// Probe each `@lib(...)` name referenced by any extern fn and
/// return the subset that loads via dlopen at build time. The CLI
/// uses this to filter `-l<missing>` flags out of the link command,
/// and aot codegen uses it to swap missing-optional fn declarations
/// to local stubs.
pub fn probe_available_libs(prog: &Program) -> std::collections::HashSet<String> {
    let mut all = std::collections::HashSet::new();
    for f in &prog.functions {
        if !matches!(f.kind, ilang_mir::FunctionKind::Extern { .. }) {
            continue;
        }
        for sym in f.libs.iter() {
            all.insert(sym.as_str().to_string());
        }
    }
    all.retain(|name| lib_loadable(name));
    all
}

/// Walk `prog` and return the set of `@optional` extern fns whose
/// every `@lib(...)` failed to probe. Those fns must be emitted as
/// local abort-stubs so the link step doesn't complain about
/// unresolved symbols.
pub fn missing_optional_fn_names(
    prog: &Program,
    available: &std::collections::HashSet<String>,
) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    for f in &prog.functions {
        if !matches!(f.kind, ilang_mir::FunctionKind::Extern { .. }) {
            continue;
        }
        if !f.is_optional {
            continue;
        }
        let any = f.libs.iter().any(|l| available.contains(l.as_str()));
        if any {
            continue;
        }
        // None of the fn's libs loaded — record by its C-side name
        // so the codegen path can match against `Function.c_symbol`
        // when picking Linkage::Local instead of Import.
        let sym = f
            .c_symbol
            .as_ref()
            .map(|s| s.as_str().to_string())
            .unwrap_or_else(|| f.name.as_str().to_string());
        out.insert(sym);
    }
    out
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
    // AOT output is a build artifact, so always run cranelift's
    // optimizer. Codegen-time cost is one-shot vs. every-run-after,
    // so prefer speed over compile latency unconditionally.
    flag_builder
        .set("opt_level", "speed")
        .map_err(|e| AotError::Other(format!("set opt_level: {e}")))?;
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

    // Probe `@lib(...)` names so we know which won't resolve at
    // link time. `@optional` extern fns whose libs all fail get
    // declared as local abort-stubs below; the CLI uses the same
    // probe (via `probe_available_libs`) to filter `-l` flags.
    let available_libs = probe_available_libs(prog);
    let missing_optional = missing_optional_fn_names(prog, &available_libs);

    // Shared lowering pass: declares every user fn (and the full
    // runtime-symbol import set), pre-defines string-literal data,
    // and lowers every fn body to clif IR. Returns the FuncId map for
    // the entry-wrapping step below.
    let outputs = lower_program_into_with_missing(
        &mut module,
        prog,
        &[],
        &class_global,
        &enum_global,
        &missing_optional,
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

    // Emit abort-stub bodies for `@optional` extern fns whose
    // `@lib(...)` libraries all failed to probe at build time. The
    // stub calls `__ilang_panic` so any actual invocation aborts —
    // user code is expected to gate via `os.libLoaded(...)` first,
    // and the link step is happy because the symbol is now Local.
    if !outputs.missing_optional_fn_ids.is_empty() {
        let panic_sig = {
            let mut s = module.make_signature();
            s.params.push(AbiParam::new(types::I64));
            s
        };
        let panic_fn = module
            .declare_function("__ilang_panic", Linkage::Import, &panic_sig)?;
        let msg_did = declare_ilang_string_data(
            &mut module,
            "@optional fn invoked but its lib failed to load",
        )?;
        for &mid in &outputs.missing_optional_fn_ids {
            let cid = *outputs.fn_ids.get(&mid).expect("missing optional fn id");
            let sig = module.declarations().get_function_decl(cid).signature.clone();
            ctx.func = ClifFunc::with_name_signature(
                UserFuncName::user(0, cid.as_u32()),
                sig,
            );
            {
                let mut fb = ClifFnBuilder::new(&mut ctx.func, &mut fb_ctx);
                let block = fb.create_block();
                // Mirror the actual signature so cranelift sees
                // matching block params for indirect call edges.
                let sig_clone = fb.func.signature.clone();
                for p in &sig_clone.params {
                    fb.append_block_param(block, p.value_type);
                }
                fb.switch_to_block(block);
                fb.seal_block(block);
                let pfn = module.declare_func_in_func(panic_fn, fb.func);
                let body = ilang_string_body(&mut module, &mut fb, msg_did);
                fb.ins().call(pfn, &[body]);
                // After panic we need a terminator. Emit a Return with
                // zero values for any expected return type.
                let mut rets: Vec<cranelift::prelude::Value> = Vec::new();
                let ret_types: Vec<_> =
                    sig_clone.returns.iter().map(|p| p.value_type).collect();
                for ty in ret_types {
                    rets.push(if ty.is_int() {
                        fb.ins().iconst(ty, 0)
                    } else if ty == types::F32 {
                        fb.ins().f32const(0.0)
                    } else if ty == types::F64 {
                        fb.ins().f64const(0.0)
                    } else {
                        fb.ins().iconst(types::I64, 0)
                    });
                }
                fb.ins().return_(&rets);
                fb.finalize();
            }
            module
                .define_function(cid, &mut ctx)
                .map_err(|e| AotError::Other(format!(
                    "define_function optional-stub `{}`: {e:?}",
                    prog.functions[mid.0 as usize].name.as_str()
                )))?;
            module.clear_context(&mut ctx);
        }
    }

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
        // Drain the Promise / pool tasks the program scheduled so
        // pending `.then` / executor bodies actually run before
        // `main` returns. No-op if the user never touched a Promise.
        let drain_sig = module.make_signature();
        let drain_id = module.declare_function(
            "__promise_drain",
            Linkage::Import,
            &drain_sig,
        )?;
        let drain_ref = module.declare_func_in_func(drain_id, fb.func);
        fb.ins().call(drain_ref, &[]);
        fb.ins().return_(&[ret32]);
        fb.finalize();
    }
    module
        .define_function(main_id, &mut ctx)
        .map_err(|e| AotError::Other(format!("define_function main: {e:?}")))?;

    // `mut` is only consumed by the macOS-gated block below
    // (`set_macho_build_version`); on Windows / Linux it'd be an
    // unused-mut warning otherwise, hence the `#[allow]`.
    #[allow(unused_mut)]
    let mut product = module.finish();
    // Embed an `LC_BUILD_VERSION` load command in the Mach-O output
    // when targeting macOS. Without it the system linker prints
    // "no platform load command found ... assuming: macOS" at every
    // link. The version encoding is nibble-packed `xxxx.yy.zz`; we
    // emit `11.0.0` as a safe arm64-compatible floor.
    #[cfg(target_os = "macos")]
    {
        use object::macho::PLATFORM_MACOS;
        use object::write::MachOBuildVersion;
        let mut bv = MachOBuildVersion::default();
        bv.platform = PLATFORM_MACOS;
        bv.minos = 11 << 16;
        bv.sdk = 11 << 16;
        product.object.set_macho_build_version(bv);
    }
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
    let reg_struct_print_field = {
        let mut s = module.make_signature();
        s.params.push(AbiParam::new(types::I64));
        s.params.push(AbiParam::new(types::I64));
        s.params.push(AbiParam::new(types::I64));
        s.params.push(AbiParam::new(types::I64));
        s.params.push(AbiParam::new(types::I64));
        s.params.push(AbiParam::new(types::I64));
        module.declare_function("__register_struct_print_field", Linkage::Import, &s)?
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
    let reg_fn_name = {
        let mut s = module.make_signature();
        s.params.push(AbiParam::new(types::I64));
        s.params.push(AbiParam::new(types::I64));
        module.declare_function("__register_fn_name", Linkage::Import, &s)?
    };
    let reg_enum_disc_str = {
        let mut s = module.make_signature();
        s.params.push(AbiParam::new(types::I64));
        s.params.push(AbiParam::new(types::I64));
        s.params.push(AbiParam::new(types::I64));
        module.declare_function("__register_enum_disc_str", Linkage::Import, &s)?
    };
    let reg_lib_group_begin = {
        let mut s = module.make_signature();
        s.returns.push(AbiParam::new(types::I64));
        module.declare_function("__register_lib_group_begin", Linkage::Import, &s)?
    };
    let reg_lib_group_member = {
        let mut s = module.make_signature();
        s.params.push(AbiParam::new(types::I64));
        s.params.push(AbiParam::new(types::I64));
        module.declare_function("__register_lib_group_member", Linkage::Import, &s)?
    };

    // Scan the whole program for `MirTy::Weak(C)` references so the
    // class-size registration below can skip those classes — `weak.get`
    // peeks at the object header after the strong rc hits zero, so
    // freeing the buffer there would dangle.
    let weakable_classes: std::collections::HashSet<u32> = {
        let mut set = std::collections::HashSet::new();
        fn walk(ty: &MirTy, set: &mut std::collections::HashSet<u32>) {
            if let MirTy::Weak(c) = ty {
                set.insert(c.0);
            }
            match ty {
                MirTy::Array { elem, .. } => walk(elem, set),
                MirTy::Optional(inner) => walk(inner, set),
                MirTy::Tuple(items) => {
                    for t in items.iter() {
                        walk(t, set);
                    }
                }
                MirTy::Map { key, val } => {
                    walk(key, set);
                    walk(val, set);
                }
                MirTy::Fn(ft) => {
                    for p in ft.params.iter() {
                        walk(p, set);
                    }
                    walk(&ft.ret, set);
                }
                _ => {}
            }
        }
        for class in &prog.classes {
            for f in &class.fields {
                walk(&f.ty, &mut set);
            }
        }
        for f in &prog.functions {
            for p in f.params.iter() {
                walk(&p.ty, &mut set);
            }
            walk(&f.ret, &mut set);
            for l in f.value_tys.iter() {
                walk(l, &mut set);
            }
            for l in f.local_tys.iter() {
                walk(l, &mut set);
            }
        }
        set
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
    // For `__register_fn_name`: data symbols holding the user-facing
    // (un-mangled) name of every non-extern user fn, keyed by the
    // same FuncId index used in `fn_ids`. `None` for synthetic /
    // anon / `__main` / extern (those don't get a printable name or
    // would force the linker to resolve a non-existent symbol).
    let mut fn_display_name_data: Vec<Option<DataId>> =
        Vec::with_capacity(prog.functions.len());
    for func in prog.functions.iter() {
        if matches!(func.kind, ilang_mir::FunctionKind::Extern { .. }) {
            fn_display_name_data.push(None);
            continue;
        }
        let name = func.name.as_str();
        if name.starts_with("__anon_fn_") || name.starts_with("__main") {
            fn_display_name_data.push(None);
            continue;
        }
        let plain = name.split("__").next().unwrap_or(name);
        fn_display_name_data
            .push(Some(declare_ilang_string_data(module, plain)?));
    }
    let mut enum_name_data: Vec<DataId> = Vec::with_capacity(prog.enums.len());
    let mut enum_variant_name_data: Vec<Vec<DataId>> =
        Vec::with_capacity(prog.enums.len());
    // For `: string`-repr enums: data symbol per (enum, variant) for
    // the discriminant string used by `enum as string` casts.
    let mut enum_variant_disc_str_data: Vec<Vec<Option<DataId>>> =
        Vec::with_capacity(prog.enums.len());
    for e in &prog.enums {
        enum_name_data.push(declare_ilang_string_data(module, e.name.as_str())?);
        let is_str_repr = matches!(e.repr, MirTy::Str);
        let mut per_enum = Vec::with_capacity(e.variants.len());
        let mut per_enum_disc: Vec<Option<DataId>> = Vec::with_capacity(e.variants.len());
        for v in &e.variants {
            per_enum.push(declare_ilang_string_data(module, v.name.as_str())?);
            if is_str_repr {
                if let Some(s) = v.discriminant_str.as_ref() {
                    per_enum_disc.push(Some(declare_ilang_string_data(module, s)?));
                } else {
                    per_enum_disc.push(None);
                }
            } else {
                per_enum_disc.push(None);
            }
        }
        enum_variant_name_data.push(per_enum);
        enum_variant_disc_str_data.push(per_enum_disc);
    }

    // Pre-allocate one data symbol per name inside every `@lib(a, b,
    // ...)` fallback group so `os.libLoaded` can be queried at
    // runtime. Stored as `Vec<Vec<DataId>>` matching the `(group,
    // member)` indices used in the init body below.
    let mut lib_group_data: Vec<Vec<DataId>> = Vec::new();
    for f in prog.functions.iter() {
        if matches!(f.kind, ilang_mir::FunctionKind::Extern { .. }) && f.libs.len() > 1 {
            let mut members = Vec::with_capacity(f.libs.len());
            for sym in f.libs.iter() {
                members.push(declare_ilang_string_data(module, sym.as_str())?);
            }
            lib_group_data.push(members);
        }
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
        let reg_struct_print_field_ref =
            module.declare_func_in_func(reg_struct_print_field, fb.func);
        let reg_enum_print_name_ref =
            module.declare_func_in_func(reg_enum_print_name, fb.func);
        let reg_enum_print_variant_name_ref =
            module.declare_func_in_func(reg_enum_print_variant_name, fb.func);
        let reg_enum_print_variant_payload_pk_ref =
            module.declare_func_in_func(reg_enum_print_variant_payload_pk, fb.func);
        let reg_fn_name_ref =
            module.declare_func_in_func(reg_fn_name, fb.func);
        let reg_enum_disc_str_ref =
            module.declare_func_in_func(reg_enum_disc_str, fb.func);
        let reg_lib_group_begin_ref =
            module.declare_func_in_func(reg_lib_group_begin, fb.func);
        let reg_lib_group_member_ref =
            module.declare_func_in_func(reg_lib_group_member, fb.func);

        for (cls_idx, class) in prog.classes.iter().enumerate() {
            let global_cid = class_global[class.id.0 as usize] as i64;
            // Print info: class name + per-field (name, PK_*).
            let cid_v = fb.ins().iconst(types::I64, global_cid);
            let name_did = class_name_data[cls_idx];
            let name_body = ilang_string_body(module, &mut fb, name_did);
            fb.ins().call(reg_class_print_name_ref, &[cid_v, name_body]);
            let is_struct = matches!(
                class.repr,
                ilang_mir::ClassRepr::CRepr
                    | ilang_mir::ClassRepr::CPacked
                    | ilang_mir::ClassRepr::CUnion
            );
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
                if is_struct && f.bit_field.is_none() {
                    let off = class.c_field_offsets.get(fi).copied().unwrap_or(0);
                    let nested_cid: i64 = if let MirTy::Object(nc) = &f.ty {
                        let nested = &prog.classes[nc.0 as usize];
                        if matches!(
                            nested.repr,
                            ilang_mir::ClassRepr::CRepr
                                | ilang_mir::ClassRepr::CPacked
                                | ilang_mir::ClassRepr::CUnion
                        ) {
                            class_global[nc.0 as usize] as i64
                        } else {
                            0
                        }
                    } else {
                        0
                    };
                    let fname_body2 = ilang_string_body(module, &mut fb, fname_did);
                    let idx_v2 = fb.ins().iconst(types::I64, fi as i64);
                    let pk_v2 = fb.ins().iconst(types::I64, pk);
                    let off_v = fb.ins().iconst(types::I64, off);
                    let nested_v = fb.ins().iconst(types::I64, nested_cid);
                    fb.ins().call(
                        reg_struct_print_field_ref,
                        &[cid_v, idx_v2, fname_body2, pk_v2, off_v, nested_v],
                    );
                }
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
            ) || weakable_classes.contains(&class.id.0);
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

        // Register the user-facing name of every non-extern fn so
        // `__print_fn` can spell out `<fn NAME>` on closure print.
        for (idx, _func) in prog.functions.iter().enumerate() {
            let did = match fn_display_name_data[idx] {
                Some(d) => d,
                None => continue,
            };
            let mid = FuncId(idx as u32);
            let cl_id = match fn_ids.get(&mid) {
                Some(c) => *c,
                None => continue,
            };
            let fr = module.declare_func_in_func(cl_id, fb.func);
            let fn_addr = fb.ins().func_addr(types::I64, fr);
            let name_body = ilang_string_body(module, &mut fb, did);
            fb.ins().call(reg_fn_name_ref, &[fn_addr, name_body]);
        }
        // Register discriminant strings for `: string`-repr enums so
        // `enum as string` casts succeed at runtime.
        for (idx, e) in prog.enums.iter().enumerate() {
            if !matches!(e.repr, MirTy::Str) {
                continue;
            }
            let global_id = enum_global[idx] as i64;
            let eid_v = fb.ins().iconst(types::I64, global_id);
            for (vi, v) in e.variants.iter().enumerate() {
                let did = match enum_variant_disc_str_data[idx][vi] {
                    Some(d) => d,
                    None => continue,
                };
                let s_body = ilang_string_body(module, &mut fb, did);
                let disc_v = fb.ins().iconst(types::I64, v.discriminant);
                fb.ins().call(reg_enum_disc_str_ref, &[eid_v, disc_v, s_body]);
            }
        }

        // Register every `@lib(...)` fallback group so the runtime's
        // `os.libLoaded(name)` can fall through to alternates.
        for members in &lib_group_data {
            let begin_call = fb.ins().call(reg_lib_group_begin_ref, &[]);
            let g = fb.inst_results(begin_call)[0];
            for did in members {
                let body = ilang_string_body(module, &mut fb, *did);
                fb.ins().call(reg_lib_group_member_ref, &[g, body]);
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

