//! Compile a MIR `Program` into a Cranelift JIT module and invoke
//! the entry function.
//!
//! Currently restricted to programs whose values are all primitive
//! scalars (integers / floats / bool / unit). Heap, ARC, FFI, and
//! virtual dispatch land alongside their MIR features in follow-up
//! steps.

use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::{AbiParam, Function as ClifFunc, InstBuilder, Signature, UserFuncName};
use cranelift_codegen::settings;
use cranelift_frontend::{FunctionBuilder as ClifFnBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module};

use ilang_ast::Symbol;
use ilang_mir::{
    BinOp, ClassId, FuncId, FuncRef, Function as MirFunction, Inst, MirConst, MirTy,
    Program, StaticSlotId, Terminator, UnOp, ValueId,
};

#[derive(Clone, Copy)]
struct MapIds {
    new: cranelift_module::FuncId,
    get: cranelift_module::FuncId,
    get_optional: cranelift_module::FuncId,
    set: cranelift_module::FuncId,
    size: cranelift_module::FuncId,
    has: cranelift_module::FuncId,
    delete: cranelift_module::FuncId,
    keys: cranelift_module::FuncId,
    values: cranelift_module::FuncId,
}

#[derive(Clone, Copy)]
struct StrIds {
    length: cranelift_module::FuncId,
    concat: cranelift_module::FuncId,
    eq: cranelift_module::FuncId,
    int_to_string: cranelift_module::FuncId,
    bool_to_string: cranelift_module::FuncId,
    to_upper: cranelift_module::FuncId,
    to_lower: cranelift_module::FuncId,
    trim: cranelift_module::FuncId,
    includes: cranelift_module::FuncId,
    starts_with: cranelift_module::FuncId,
    ends_with: cranelift_module::FuncId,
    char_at: cranelift_module::FuncId,
    slice: cranelift_module::FuncId,
    replace: cranelift_module::FuncId,
    array_index_of: cranelift_module::FuncId,
    array_includes: cranelift_module::FuncId,
    array_push: cranelift_module::FuncId,
    array_pop: cranelift_module::FuncId,
    array_map: cranelift_module::FuncId,
    array_filter: cranelift_module::FuncId,
    array_for_each: cranelift_module::FuncId,
    array_slice: cranelift_module::FuncId,
    str_split: cranelift_module::FuncId,
    virt_dispatch: cranelift_module::FuncId,
    fixed_to_dyn: cranelift_module::FuncId,
}

fn declare_unary_i64(
    module: &mut JITModule,
    name: &str,
) -> Result<cranelift_module::FuncId, CompileError> {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(types::I64));
    Ok(module.declare_function(name, Linkage::Import, &sig)?)
}

fn declare_binary_i64(
    module: &mut JITModule,
    name: &str,
) -> Result<cranelift_module::FuncId, CompileError> {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64));
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(types::I64));
    Ok(module.declare_function(name, Linkage::Import, &sig)?)
}

fn declare_ternary_i64(
    module: &mut JITModule,
    name: &str,
) -> Result<cranelift_module::FuncId, CompileError> {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64));
    sig.params.push(AbiParam::new(types::I64));
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(types::I64));
    Ok(module.declare_function(name, Linkage::Import, &sig)?)
}

fn declare_unit_i64(
    module: &mut JITModule,
    name: &str,
) -> Result<cranelift_module::FuncId, CompileError> {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64));
    Ok(module.declare_function(name, Linkage::Import, &sig)?)
}

fn declare_unit_f64(
    module: &mut JITModule,
    name: &str,
) -> Result<cranelift_module::FuncId, CompileError> {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::F64));
    Ok(module.declare_function(name, Linkage::Import, &sig)?)
}

fn declare_unit_void(
    module: &mut JITModule,
    name: &str,
) -> Result<cranelift_module::FuncId, CompileError> {
    let sig = module.make_signature();
    Ok(module.declare_function(name, Linkage::Import, &sig)?)
}

#[derive(Clone, Copy)]
struct PrintIds {
    int: cranelift_module::FuncId,
    bool_: cranelift_module::FuncId,
    f64_: cranelift_module::FuncId,
    str_: cranelift_module::FuncId,
    space: cranelift_module::FuncId,
    newline: cranelift_module::FuncId,
    object: cranelift_module::FuncId,
    fn_: cranelift_module::FuncId,
    map: cranelift_module::FuncId,
    weak: cranelift_module::FuncId,
    enum_: cranelift_module::FuncId,
}

#[derive(Clone, Copy)]
#[allow(dead_code)] // `drop_dispatch` / `print_map` are only consumed via runtime symbol resolution today, but kept on this aggregate so future codegen sites can reach for them without re-plumbing.
struct PanicAux {
    fn_id: cranelift_module::FuncId,
    drop_dispatch: cranelift_module::FuncId,
    release_obj: cranelift_module::FuncId,
    retain_obj: cranelift_module::FuncId,
    release_closure: cranelift_module::FuncId,
    retain_closure: cranelift_module::FuncId,
    release_array: cranelift_module::FuncId,
    retain_array: cranelift_module::FuncId,
    release_optional: cranelift_module::FuncId,
    retain_optional: cranelift_module::FuncId,
    release_tuple: cranelift_module::FuncId,
    retain_tuple: cranelift_module::FuncId,
    release_map: cranelift_module::FuncId,
    retain_map: cranelift_module::FuncId,
    map_set_obj_val: cranelift_module::FuncId,
    map_set_print_kinds: cranelift_module::FuncId,
    print_map: cranelift_module::FuncId,
    class_name: cranelift_module::FuncId,
    /// `__mir_free(ptr, size)` — drops a previously-`mir_alloc`'d
    /// block. Used by `Inst::Release` for CRepr structs (which
    /// have no rc header but still need their backing buffer
    /// freed when they fall out of scope).
    mir_free: cranelift_module::FuncId,
    release_string: cranelift_module::FuncId,
    retain_string: cranelift_module::FuncId,
    enum_unit_get: cranelift_module::FuncId,
    enum_alloc: cranelift_module::FuncId,
    release_enum: cranelift_module::FuncId,
    retain_enum: cranelift_module::FuncId,
    msg_div: DataId,
    msg_mod: DataId,
    msg_oob: DataId,
    msg_unwrap: DataId,
}

#[derive(Clone, Copy)]
struct PrintLits {
    none: DataId,
    some_open: DataId,
    close_paren: DataId,
    open_paren: DataId,
    open_bracket: DataId,
    close_bracket: DataId,
    comma_sp: DataId,
}

/// Bytes prepended to every heap object: holds the `ClassId` so RTTI
/// (`is_instance`, `as?`, `typeof`) can recover the dynamic class.
const OBJECT_HEADER_BYTES: i32 = 16;
use cranelift_frontend::Variable;
use cranelift_module::{DataDescription, DataId};

use crate::ty::mir_to_clif;

#[derive(Debug, thiserror::Error)]
pub enum CompileError {
    #[error("unsupported in M1: {0}")]
    Unsupported(&'static str),
    #[error("{0}")]
    Other(String),
    #[error(transparent)]
    Module(#[from] cranelift_module::ModuleError),
}

/// Compile a MIR program and return the constructed JIT module plus
/// a handle to the entry fn (`__main`-equivalent — user code's tail
/// expression).
pub struct Compiled {
    pub module: JITModule,
    pub entry: cranelift_module::FuncId,
    pub entry_ret: MirTy,
}

/// Description of a host-provided builtin function. The JIT links the
/// symbol against `ptr`, and `params` / `ret` describe the C-ABI
/// signature seen by `Inst::Call(FuncRef::Builtin)` sites.
pub struct BuiltinDecl {
    pub name: &'static str,
    pub params: Vec<MirTy>,
    pub ret: MirTy,
    pub ptr: *const u8,
}

pub fn compile_program(prog: &Program) -> Result<Compiled, CompileError> {
    compile_with_builtins(prog, &[])
}

/// Like `compile_program`, but registers host-provided builtins. Call
/// sites that name `Inst::Call(FuncRef::Builtin(sym))` resolve to
/// these.
pub fn compile_with_builtins(
    prog: &Program,
    builtins: &[BuiltinDecl],
) -> Result<Compiled, CompileError> {
    let isa_builder = cranelift_native::builder()
        .map_err(|e| CompileError::Other(format!("cranelift_native: {e}")))?;
    let flag_builder = settings::builder();
    let isa = isa_builder
        .finish(settings::Flags::new(flag_builder))
        .map_err(|e| CompileError::Other(format!("isa: {e}")))?;

    // Allocate globally-unique class / enum ids for this compile so
    // its runtime tables don't collide with any other module's. The
    // GLOBAL id is what gets stored into heap-object headers, and
    // every host table lookup keys off it.
    let class_global: Vec<u32> = (0..prog.classes.len())
        .map(|_| alloc_global_class_id())
        .collect();
    let enum_global: Vec<u32> = (0..prog.enums.len())
        .map(|_| alloc_global_enum_id())
        .collect();
    let global_cid = |local: u32| class_global[local as usize];
    let global_eid = |local: u32| enum_global[local as usize];

    // Pre-build a per-(global_class_id, slot) → method-fn-id map from
    // the MIR. The actual function addresses are filled in after
    // `finalize_definitions()` and exposed to JIT code via the
    // `__virt_dispatch` host helper.
    let mut vtable_entries: HashMap<(u32, u32), FuncId> = HashMap::new();
    for class in &prog.classes {
        for m in &class.methods {
            if let Some(slot) = m.slot {
                vtable_entries.insert((global_cid(class.id.0), slot.0), m.func);
            }
        }
    }

    let mut jit_builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    // Always-available allocator. Allocates `size` bytes (zero-init)
    // via Rust's `Vec<u8>` and leaks the pointer. The MIR codegen's
    // ARC step is what eventually frees it; until then it's a small
    // intentional leak that's fine for short-running test programs.
    jit_builder.symbol("__mir_alloc", host_mir_alloc as *const u8);
    jit_builder.symbol("__mir_free", host_mir_free as *const u8);
    // Map runtime backed by Rust's HashMap<i64, i64> (one box per
    // map). Keys / values flow through as i64 cells (heap pointers
    // share identity when interned).
    jit_builder.symbol("__map_new", host_map_new as *const u8);
    jit_builder.symbol("__map_get", host_map_get as *const u8);
    jit_builder.symbol("__map_get_optional", host_map_get_optional as *const u8);
    jit_builder.symbol("__map_set", host_map_set as *const u8);
    jit_builder.symbol("__map_has", host_map_has as *const u8);
    jit_builder.symbol("__map_size", host_map_size as *const u8);
    jit_builder.symbol("__map_delete", host_map_delete as *const u8);
    jit_builder.symbol("__map_keys", host_map_keys as *const u8);
    jit_builder.symbol("__map_values", host_map_values as *const u8);
    // Default string builtins. Returns are NUL-terminated `*const u8`
    // pointers to leaked Rust-side allocations. Acceptable until the
    // ARC-backed StringRc runtime arrives.
    jit_builder.symbol("__str_length", host_str_length as *const u8);
    jit_builder.symbol("__str_concat", host_str_concat as *const u8);
    jit_builder.symbol("__str_eq", host_str_eq as *const u8);
    jit_builder.symbol("__int_to_string", host_int_to_string as *const u8);
    jit_builder.symbol("__bool_to_string", host_bool_to_string as *const u8);
    jit_builder.symbol("__str_to_upper", host_str_to_upper as *const u8);
    jit_builder.symbol("__str_to_lower", host_str_to_lower as *const u8);
    jit_builder.symbol("__str_trim", host_str_trim as *const u8);
    jit_builder.symbol("__str_includes", host_str_includes as *const u8);
    jit_builder.symbol("__str_starts_with", host_str_starts_with as *const u8);
    jit_builder.symbol("__str_ends_with", host_str_ends_with as *const u8);
    jit_builder.symbol("__str_char_at", host_str_char_at as *const u8);
    jit_builder.symbol("__str_slice", host_str_slice as *const u8);
    jit_builder.symbol("__str_replace", host_str_replace as *const u8);
    jit_builder.symbol("__array_index_of", host_array_index_of as *const u8);
    jit_builder.symbol("__array_includes", host_array_includes as *const u8);
    jit_builder.symbol("__array_push", host_array_push as *const u8);
    jit_builder.symbol("__array_pop", host_array_pop as *const u8);
    jit_builder.symbol("__fixed_to_dyn", host_fixed_to_dyn as *const u8);
    jit_builder.symbol("__enum_box", host_enum_box as *const u8);
    jit_builder.symbol("__c_array_to_array", host_c_array_to_array as *const u8);
    jit_builder.symbol("__repl_load_slot", host_repl_load_slot as *const u8);
    jit_builder.symbol("__repl_store_slot", host_repl_store_slot as *const u8);
    // Raw-memory FFI marshalling: `readT(p, off): T` / `writeT(p,
    // off, v)`. The `read*` family folds the loaded primitive to
    // i64 (or f32/f64) for the cross-FFI return; callers reinterpret
    // via the slot-typing handled in `lower_call`.
    jit_builder.symbol("__read_i8", host_read_i8 as *const u8);
    jit_builder.symbol("__read_i16", host_read_i16 as *const u8);
    jit_builder.symbol("__read_i32", host_read_i32 as *const u8);
    jit_builder.symbol("__read_i64", host_read_i64 as *const u8);
    jit_builder.symbol("__read_u8", host_read_u8 as *const u8);
    jit_builder.symbol("__read_u16", host_read_u16 as *const u8);
    jit_builder.symbol("__read_u32", host_read_u32 as *const u8);
    jit_builder.symbol("__read_u64", host_read_u64 as *const u8);
    jit_builder.symbol("__read_f32", host_read_f32 as *const u8);
    jit_builder.symbol("__read_f64", host_read_f64 as *const u8);
    jit_builder.symbol("__write_i8", host_write_i8 as *const u8);
    jit_builder.symbol("__write_i16", host_write_i16 as *const u8);
    jit_builder.symbol("__write_i32", host_write_i32 as *const u8);
    jit_builder.symbol("__write_i64", host_write_i64 as *const u8);
    jit_builder.symbol("__write_u8", host_write_u8 as *const u8);
    jit_builder.symbol("__write_u16", host_write_u16 as *const u8);
    jit_builder.symbol("__write_u32", host_write_u32 as *const u8);
    jit_builder.symbol("__write_u64", host_write_u64 as *const u8);
    jit_builder.symbol("__write_f32", host_write_f32 as *const u8);
    jit_builder.symbol("__write_f64", host_write_f64 as *const u8);
    jit_builder.symbol("__array_map", host_array_map as *const u8);
    jit_builder.symbol("__array_filter", host_array_filter as *const u8);
    jit_builder.symbol("__array_for_each", host_array_for_each as *const u8);
    jit_builder.symbol("__array_slice", host_array_slice as *const u8);
    jit_builder.symbol("__str_split", host_str_split as *const u8);
    jit_builder.symbol("__virt_dispatch", host_virt_dispatch as *const u8);
    jit_builder.symbol("__drop_dispatch", host_drop_dispatch as *const u8);
    jit_builder.symbol("__print_object", host_print_object as *const u8);
    jit_builder.symbol("__class_name", host_class_name as *const u8);
    jit_builder.symbol("__print_weak", host_print_weak as *const u8);
    jit_builder.symbol("__print_enum", host_print_enum as *const u8);
    jit_builder.symbol("__print_fn", host_print_fn as *const u8);
    jit_builder.symbol("__release_object", host_release_object as *const u8);
    jit_builder.symbol("__retain_object", host_retain_object as *const u8);
    jit_builder.symbol("__release_closure", host_release_closure as *const u8);
    jit_builder.symbol("__retain_closure", host_retain_closure as *const u8);
    jit_builder.symbol("__release_array", host_release_array as *const u8);
    jit_builder.symbol("__retain_array", host_retain_array as *const u8);
    jit_builder.symbol("__release_optional", host_release_optional as *const u8);
    jit_builder.symbol("__retain_optional", host_retain_optional as *const u8);
    jit_builder.symbol("__release_tuple", host_release_tuple as *const u8);
    jit_builder.symbol("__retain_tuple", host_retain_tuple as *const u8);
    jit_builder.symbol("__release_map", host_release_map as *const u8);
    jit_builder.symbol("__retain_map", host_retain_map as *const u8);
    jit_builder.symbol("__release_string", host_release_string as *const u8);
    jit_builder.symbol("__retain_string", host_retain_string as *const u8);
    // Always-on memory-tracking helpers exposed through `test.liveAlloc*`
    // / `test.liveStringCount`. Used by the leak-detection fixtures
    // under tests/programs/.
    jit_builder.symbol("test.liveAllocBytes", host_test_live_alloc_bytes as *const u8);
    jit_builder.symbol("test.liveAllocCount", host_test_live_alloc_count as *const u8);
    jit_builder.symbol("test.liveStringCount", host_test_live_string_count as *const u8);
    jit_builder.symbol("__enum_alloc", host_enum_alloc as *const u8);
    jit_builder.symbol("__release_enum", host_release_enum as *const u8);
    jit_builder.symbol("__retain_enum", host_retain_enum as *const u8);
    jit_builder.symbol("__enum_unit_get", host_enum_unit_get as *const u8);
    jit_builder.symbol("__map_set_object_value", host_map_set_object_value as *const u8);
    jit_builder.symbol("__map_set_print_kinds", host_map_set_print_kinds as *const u8);
    jit_builder.symbol("__print_map", host_print_map as *const u8);
    // FFI marshalling helpers — registered both with their bare names
    // (used inside `@extern(C)` blocks) and qualified names. Strings
    // are NUL-terminated `*const u8` already, so most "C-string"
    // helpers are identity at the bit level.
    jit_builder.symbol("__array_data_ptr", host_array_data_ptr as *const u8);
    jit_builder.symbol("cstrFromString", host_identity as *const u8);
    jit_builder.symbol("stringFromCstr", host_string_from_cstr as *const u8);
    jit_builder.symbol("cstrArrayToStrings", host_cstr_array_to_strings as *const u8);
    jit_builder.symbol("freeCstr", host_noop as *const u8);
    jit_builder.symbol("errnoCheck", host_errno_check_i32 as *const u8);
    jit_builder.symbol("errnoCheckI64", host_errno_check_i64 as *const u8);
    jit_builder.symbol("os.errno", host_os_errno as *const u8);
    jit_builder.symbol("os.setErrno", host_os_set_errno as *const u8);
    jit_builder.symbol("os.libLoaded", host_os_lib_loaded as *const u8);
    jit_builder.symbol("os.libLoadError", host_os_lib_load_error as *const u8);
    // Built-in `test.*` runtime — fixture programs use these to
    // self-check. Failures abort the process with exit code 2.
    // Reuse the legacy JIT's full test-extern symbol set (callbacks,
    // by-value structs, sret returns, errno helpers, etc), then
    // override the closure-callback shim with our mir-aware one
    // since the legacy version expects a raw fn pointer and would
    // jump to a heap-data address otherwise.
    ilang_codegen::test_externs::register_test_symbols(&mut jit_builder);
    jit_builder.symbol("test.applyI32Cb", host_test_apply_i32_cb as *const u8);
    jit_builder.symbol("test.expect", host_test_expect as *const u8);
    jit_builder.symbol("test.expectStr", host_test_expect_str as *const u8);
    jit_builder.symbol("test.expectBool", host_test_expect_bool as *const u8);
    jit_builder.symbol("test.expectF64", host_test_expect_f64 as *const u8);
    jit_builder.symbol("test.expectTrue", host_test_expect_true as *const u8);
    jit_builder.symbol("test.expectFalse", host_test_expect_false as *const u8);
    jit_builder.symbol("test.fail", host_test_fail as *const u8);
    // Built-in `math.*` runtime — wraps `f64::*` Rust intrinsics.
    jit_builder.symbol("math.sin", host_sin as *const u8);
    jit_builder.symbol("math.cos", host_cos as *const u8);
    jit_builder.symbol("math.tan", host_tan as *const u8);
    jit_builder.symbol("math.asin", host_asin as *const u8);
    jit_builder.symbol("math.acos", host_acos as *const u8);
    jit_builder.symbol("math.atan", host_atan as *const u8);
    jit_builder.symbol("math.atan2", host_atan2 as *const u8);
    jit_builder.symbol("math.sqrt", host_sqrt as *const u8);
    jit_builder.symbol("math.pow", host_pow as *const u8);
    jit_builder.symbol("math.exp", host_exp as *const u8);
    jit_builder.symbol("math.ln", host_ln as *const u8);
    jit_builder.symbol("math.log10", host_log10 as *const u8);
    jit_builder.symbol("math.log2", host_log2 as *const u8);
    jit_builder.symbol("math.floor", host_floor as *const u8);
    jit_builder.symbol("math.ceil", host_ceil as *const u8);
    jit_builder.symbol("math.round", host_round as *const u8);
    jit_builder.symbol("math.abs", host_abs as *const u8);
    // `console.log` is variadic at the language surface, so the
    // codegen splits each argument into a per-type print call.
    jit_builder.symbol("__ilang_panic", host_ilang_panic as *const u8);
    jit_builder.symbol("__print_int", host_print_int as *const u8);
    jit_builder.symbol("__print_bool", host_print_bool as *const u8);
    jit_builder.symbol("__print_f64", host_print_f64 as *const u8);
    jit_builder.symbol("__print_str", host_print_str as *const u8);
    jit_builder.symbol("__print_space", host_print_space as *const u8);
    jit_builder.symbol("__print_newline", host_print_newline as *const u8);
    for b in builtins {
        jit_builder.symbol(b.name, b.ptr);
    }
    // Eagerly dlopen every `@lib(...)` library declared anywhere
    // in the program. Without this, dlsym(RTLD_DEFAULT, "SDL_Init")
    // misses because the dynamic loader hasn't pulled libSDL2 into
    // the process yet. Handles are leaked via Box::leak so the
    // libs stay live for the JIT's lifetime.
    {
        let mut tried: std::collections::HashSet<String> = std::collections::HashSet::new();
        for f in &prog.functions {
            if !matches!(f.kind, ilang_mir::FunctionKind::Extern { .. }) {
                continue;
            }
            for lib in &f.libs {
                let name = lib.as_str().to_string();
                if !tried.insert(name.clone()) {
                    continue;
                }
                let _ = try_open_lib(&name);
            }
        }
    }
    // For every `@extern(C) @optional` fn whose target dlsym would
    // fail (the lib couldn't be opened, or the symbol is missing),
    // bind a host stub so finalize doesn't panic. The stub aborts
    // when actually invoked — callers are expected to gate via
    // `os.libLoaded(...)` first.
    // Collect `@lib(...)` groups so `os.libLoaded(name)` can fall
    // through to alternates declared on the same fn.
    {
        let mut groups = lib_groups_lock().lock().expect("lib groups poisoned");
        groups.clear();
        for f in &prog.functions {
            if matches!(f.kind, ilang_mir::FunctionKind::Extern { .. })
                && f.libs.len() > 1
            {
                groups.push(f.libs.clone());
            }
        }
    }
    for f in &prog.functions {
        if !matches!(f.kind, ilang_mir::FunctionKind::Extern { .. }) {
            continue;
        }
        if !f.is_optional {
            continue;
        }
        let sym_name = f
            .c_symbol
            .as_ref()
            .map(|s| s.as_str().to_string())
            .unwrap_or_else(|| f.name.as_str().to_string());
        // Only register the stub if no real symbol exists in the
        // current process (dlsym from RTLD_DEFAULT). Otherwise the
        // JIT would prefer the stub over the real implementation.
        if !process_symbol_exists(&sym_name) {
            jit_builder.symbol(sym_name, host_optional_missing_stub as *const u8);
        }
    }
    let mut module = JITModule::new(jit_builder);

    // Declare the alloc builtin so NewObject can call it.
    let alloc_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        module.declare_function("__mir_alloc", Linkage::Import, &sig)?
    };
    let free_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        module.declare_function("__mir_free", Linkage::Import, &sig)?
    };
    // Map runtime imports.
    let map_new_id = {
        let mut sig = module.make_signature();
        sig.returns.push(AbiParam::new(types::I64));
        module.declare_function("__map_new", Linkage::Import, &sig)?
    };
    let map_get_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        module.declare_function("__map_get", Linkage::Import, &sig)?
    };
    let map_get_optional_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        module.declare_function("__map_get_optional", Linkage::Import, &sig)?
    };
    let map_set_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        module.declare_function("__map_set", Linkage::Import, &sig)?
    };
    let map_has_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        module.declare_function("__map_has", Linkage::Import, &sig)?
    };
    let map_size_id = declare_unary_i64(&mut module, "__map_size")?;
    let map_delete_id = declare_binary_i64(&mut module, "__map_delete")?;
    let map_keys_id = declare_unary_i64(&mut module, "__map_keys")?;
    let map_values_id = declare_unary_i64(&mut module, "__map_values")?;
    // FFI marshalling helpers as imports.
    {
        let mut decl_unary = |name: &str, ret_unit: bool| -> Result<(), CompileError> {
            let mut sig = module.make_signature();
            sig.params.push(AbiParam::new(types::I64));
            if !ret_unit {
                sig.returns.push(AbiParam::new(types::I64));
            }
            module.declare_function(name, Linkage::Import, &sig)?;
            Ok(())
        };
        decl_unary("__array_data_ptr", false)?;
        decl_unary("__enum_box", false)?;
        decl_unary("cstrFromString", false)?;
        decl_unary("stringFromCstr", false)?;
        decl_unary("cstrArrayToStrings", false)?;
        decl_unary("freeCstr", true)?;
        decl_unary("errnoCheck", false)?;
        decl_unary("errnoCheckI64", false)?;
        // os.errno / os.setErrno are declared by the user's @extern(C)
        // block (the `os` stdlib); we just register the host symbols.
    }
    {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        module.declare_function("__c_array_to_array", Linkage::Import, &sig)?;
    }
    // `read*(p: i64, off: i64) -> {i64|f32|f64}` declarations.
    for name in &[
        "__read_i8", "__read_i16", "__read_i32", "__read_i64",
        "__read_u8", "__read_u16", "__read_u32", "__read_u64",
    ] {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        module.declare_function(name, Linkage::Import, &sig)?;
    }
    {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::F32));
        module.declare_function("__read_f32", Linkage::Import, &sig)?;
    }
    {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::F64));
        module.declare_function("__read_f64", Linkage::Import, &sig)?;
    }
    // `write*(p: i64, off: i64, v: {i64|f32|f64})` declarations.
    for name in &[
        "__write_i8", "__write_i16", "__write_i32", "__write_i64",
        "__write_u8", "__write_u16", "__write_u32", "__write_u64",
    ] {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        module.declare_function(name, Linkage::Import, &sig)?;
    }
    {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::F32));
        module.declare_function("__write_f32", Linkage::Import, &sig)?;
    }
    {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::F64));
        module.declare_function("__write_f64", Linkage::Import, &sig)?;
    }
    // REPL slot accessors. Loaded as imports so chunk-level
    // compilations don't need a fresh declaration; the host symbol
    // table provides the bodies via `JITBuilder::symbol`.
    {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        module.declare_function("__repl_load_slot", Linkage::Import, &sig)?;
    }
    {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        module.declare_function("__repl_store_slot", Linkage::Import, &sig)?;
    }
    let str_ids = StrIds {
        length: declare_unary_i64(&mut module, "__str_length")?,
        concat: declare_binary_i64(&mut module, "__str_concat")?,
        eq: declare_binary_i64(&mut module, "__str_eq")?,
        int_to_string: declare_unary_i64(&mut module, "__int_to_string")?,
        bool_to_string: declare_unary_i64(&mut module, "__bool_to_string")?,
        to_upper: declare_unary_i64(&mut module, "__str_to_upper")?,
        to_lower: declare_unary_i64(&mut module, "__str_to_lower")?,
        trim: declare_unary_i64(&mut module, "__str_trim")?,
        includes: declare_binary_i64(&mut module, "__str_includes")?,
        starts_with: declare_binary_i64(&mut module, "__str_starts_with")?,
        ends_with: declare_binary_i64(&mut module, "__str_ends_with")?,
        char_at: declare_binary_i64(&mut module, "__str_char_at")?,
        slice: {
            let mut sig = module.make_signature();
            sig.params.push(AbiParam::new(types::I64));
            sig.params.push(AbiParam::new(types::I64));
            sig.params.push(AbiParam::new(types::I64));
            sig.returns.push(AbiParam::new(types::I64));
            module.declare_function("__str_slice", Linkage::Import, &sig)?
        },
        replace: {
            let mut sig = module.make_signature();
            sig.params.push(AbiParam::new(types::I64));
            sig.params.push(AbiParam::new(types::I64));
            sig.params.push(AbiParam::new(types::I64));
            sig.returns.push(AbiParam::new(types::I64));
            module.declare_function("__str_replace", Linkage::Import, &sig)?
        },
        array_index_of: declare_binary_i64(&mut module, "__array_index_of")?,
        array_includes: declare_binary_i64(&mut module, "__array_includes")?,
        array_push: {
            let mut sig = module.make_signature();
            sig.params.push(AbiParam::new(types::I64));
            sig.params.push(AbiParam::new(types::I64));
            module.declare_function("__array_push", Linkage::Import, &sig)?
        },
        array_pop: declare_unary_i64(&mut module, "__array_pop")?,
        array_map: declare_ternary_i64(&mut module, "__array_map")?,
        array_filter: declare_binary_i64(&mut module, "__array_filter")?,
        array_for_each: {
            let mut sig = module.make_signature();
            sig.params.push(AbiParam::new(types::I64));
            sig.params.push(AbiParam::new(types::I64));
            module.declare_function("__array_for_each", Linkage::Import, &sig)?
        },
        array_slice: {
            let mut sig = module.make_signature();
            sig.params.push(AbiParam::new(types::I64));
            sig.params.push(AbiParam::new(types::I64));
            sig.params.push(AbiParam::new(types::I64));
            sig.returns.push(AbiParam::new(types::I64));
            module.declare_function("__array_slice", Linkage::Import, &sig)?
        },
        str_split: declare_binary_i64(&mut module, "__str_split")?,
        virt_dispatch: declare_binary_i64(&mut module, "__virt_dispatch")?,
        fixed_to_dyn: {
            let mut sig = module.make_signature();
            sig.params.push(AbiParam::new(types::I64));
            sig.params.push(AbiParam::new(types::I64));
            sig.params.push(AbiParam::new(types::I64));
            sig.params.push(AbiParam::new(types::I64));
            sig.returns.push(AbiParam::new(types::I64));
            module.declare_function("__fixed_to_dyn", Linkage::Import, &sig)?
        },
    };
    let panic_fn_id = declare_unit_i64(&mut module, "__ilang_panic")?;
    let drop_dispatch_id = declare_unary_i64(&mut module, "__drop_dispatch")?;
    let print_object_id = declare_unit_i64(&mut module, "__print_object")?;
    let print_fn_id = declare_unit_i64(&mut module, "__print_fn")?;
    let release_obj_id = declare_unit_i64(&mut module, "__release_object")?;
    let retain_obj_id = declare_unit_i64(&mut module, "__retain_object")?;
    let release_closure_id = declare_unit_i64(&mut module, "__release_closure")?;
    let retain_closure_id = declare_unit_i64(&mut module, "__retain_closure")?;
    let release_array_id = declare_unit_i64(&mut module, "__release_array")?;
    let retain_array_id = declare_unit_i64(&mut module, "__retain_array")?;
    let release_optional_id = declare_unit_i64(&mut module, "__release_optional")?;
    let retain_optional_id = declare_unit_i64(&mut module, "__retain_optional")?;
    let release_tuple_id = declare_unit_i64(&mut module, "__release_tuple")?;
    let retain_tuple_id = declare_unit_i64(&mut module, "__retain_tuple")?;
    let release_map_id = declare_unit_i64(&mut module, "__release_map")?;
    let retain_map_id = declare_unit_i64(&mut module, "__retain_map")?;
    let release_string_id = declare_unit_i64(&mut module, "__release_string")?;
    let retain_string_id = declare_unit_i64(&mut module, "__retain_string")?;
    let enum_unit_get_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        module.declare_function("__enum_unit_get", Linkage::Import, &sig)?
    };
    let enum_alloc_id = declare_ternary_i64(&mut module, "__enum_alloc")?;
    let release_enum_id = declare_unit_i64(&mut module, "__release_enum")?;
    let retain_enum_id = declare_unit_i64(&mut module, "__retain_enum")?;
    let map_set_obj_val_id = declare_unit_i64(&mut module, "__map_set_object_value")?;
    let map_set_print_kinds_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        module.declare_function("__map_set_print_kinds", Linkage::Import, &sig)?
    };
    let print_map_id = declare_unit_i64(&mut module, "__print_map")?;
    let class_name_id = declare_unary_i64(&mut module, "__class_name")?;
    let print_weak_id = declare_unit_i64(&mut module, "__print_weak")?;
    let print_enum_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        module.declare_function("__print_enum", Linkage::Import, &sig)?
    };
    let print_ids = PrintIds {
        int: declare_unit_i64(&mut module, "__print_int")?,
        bool_: declare_unit_i64(&mut module, "__print_bool")?,
        f64_: declare_unit_f64(&mut module, "__print_f64")?,
        str_: declare_unit_i64(&mut module, "__print_str")?,
        space: declare_unit_void(&mut module, "__print_space")?,
        newline: declare_unit_void(&mut module, "__print_newline")?,
        object: print_object_id,
        fn_: print_fn_id,
        map: print_map_id,
        weak: print_weak_id,
        enum_: print_enum_id,
    };

    // Declare builtin imports. Each gets a Cranelift FuncId so call
    // sites can resolve via `module.declare_func_in_func`.
    let mut builtin_ids: HashMap<String, (cranelift_module::FuncId, Signature)> =
        HashMap::new();
    for b in builtins {
        let mut sig = module.make_signature();
        for p in &b.params {
            if let Some(ct) = mir_to_clif(p) {
                sig.params.push(AbiParam::new(ct));
            }
        }
        if !matches!(b.ret, MirTy::Unit) {
            if let Some(ct) = mir_to_clif(&b.ret) {
                sig.returns.push(AbiParam::new(ct));
            }
        }
        let cid = module.declare_function(b.name, Linkage::Import, &sig)?;
        builtin_ids.insert(b.name.to_string(), (cid, sig));
    }

    // Pre-collect every string literal in the program; each gets a
    // Cranelift data symbol laid out as
    //   [ i64 length ][ UTF-8 bytes ][ \0 ]
    // The user-visible runtime pointer points at the first byte of
    // the UTF-8 area (offset 8 from the symbol). The length prefix
    // lets `host_str_length` and friends round-trip strings that
    // contain embedded NUL bytes; the trailing NUL keeps cstr-style
    // C interop working.
    let mut string_data: HashMap<Symbol, DataId> = HashMap::new();
    let mut next_str_id: u32 = 0;
    for f in &prog.functions {
        for blk in &f.blocks {
            for inst in &blk.insts {
                if let Inst::Const { value: MirConst::Str(s), .. } = inst {
                    if !string_data.contains_key(s) {
                        let body = s.as_str().as_bytes();
                        let mut bytes: Vec<u8> = Vec::with_capacity(8 + body.len() + 1);
                        bytes.extend_from_slice(&(body.len() as i64).to_le_bytes());
                        bytes.extend_from_slice(body);
                        bytes.push(0);
                        let mut desc = DataDescription::new();
                        desc.define(bytes.into_boxed_slice());
                        let name = format!("__str_{}", next_str_id);
                        next_str_id += 1;
                        let did = module.declare_data(&name, Linkage::Local, false, false)?;
                        module.define_data(did, &desc).map_err(CompileError::Module)?;
                        string_data.insert(*s, did);
                    }
                }
            }
        }
    }

    // Pre-define panic message C-strings reused across all check
    // sites. Returns a DataId; later emitters take its address via
    // `module.declare_data_in_func`.
    // Same `[ i64 length | bytes | \0 ]` shape as user string
    // literals — keeps cstr_bytes / host_ilang_panic / host_print_str
    // happy without per-call-site special-casing. Consumers add 8 to
    // the symbol address to get the user-visible pointer.
    let mut declare_msg = |name: &str, text: &str| -> Result<DataId, CompileError> {
        let body = text.as_bytes();
        let mut bytes: Vec<u8> = Vec::with_capacity(8 + body.len() + 1);
        bytes.extend_from_slice(&(body.len() as i64).to_le_bytes());
        bytes.extend_from_slice(body);
        bytes.push(0);
        let mut desc = DataDescription::new();
        desc.define(bytes.into_boxed_slice());
        let did = module.declare_data(name, Linkage::Local, false, false)?;
        module.define_data(did, &desc).map_err(CompileError::Module)?;
        Ok(did)
    };
    let panic_msg_div = declare_msg("__panic_msg_div", "panic: division by zero")?;
    let panic_msg_mod = declare_msg("__panic_msg_mod", "panic: modulo by zero / division by zero")?;
    let panic_msg_oob = declare_msg("__panic_msg_oob", "panic: index out of bounds")?;
    let panic_msg_unwrap = declare_msg("__panic_msg_unwrap", "panic: unwrap of None")?;
    let lit_none = declare_msg("__lit_none", "none")?;
    let lit_some_open = declare_msg("__lit_some_open", "some(")?;
    let lit_close_paren = declare_msg("__lit_cparen", ")")?;
    let lit_open_paren = declare_msg("__lit_oparen", "(")?;
    let lit_open_bracket = declare_msg("__lit_obracket", "[")?;
    let lit_close_bracket = declare_msg("__lit_cbracket", "]")?;
    let lit_comma_sp = declare_msg("__lit_comma_sp", ", ")?;

    // Declare a Cranelift data symbol for every static slot. Each
    // slot occupies an i64 cell (f64 / bool stored via bitcast /
    // truncation). Initial values come from `MirConst`.
    let mut static_data: HashMap<StaticSlotId, DataId> = HashMap::new();
    for s in &prog.statics {
        let bytes = match &s.init {
            MirConst::Int(n) => (*n as i64).to_le_bytes().to_vec(),
            MirConst::Bool(b) => (if *b { 1u64 } else { 0u64 }).to_le_bytes().to_vec(),
            MirConst::F64(bits) => bits.to_le_bytes().to_vec(),
            MirConst::F32(bits) => (*bits as u64).to_le_bytes().to_vec(),
            _ => {
                return Err(CompileError::Unsupported(
                    "static slot init must be int / bool / float literal",
                ))
            }
        };
        let mut desc = DataDescription::new();
        desc.define(bytes.into_boxed_slice());
        let name = format!("__static_{}", s.id.0);
        let did = module.declare_data(&name, Linkage::Local, true, false)?;
        module.define_data(did, &desc).map_err(CompileError::Module)?;
        static_data.insert(s.id, did);
    }

    // Declare every fn first so calls can resolve in any order.
    // `FunctionKind::Extern` fns are imports — the body is supplied
    // by a host symbol registered on the JIT builder.
    let mut fn_ids: HashMap<FuncId, cranelift_module::FuncId> = HashMap::new();
    let mut fn_sigs: HashMap<FuncId, Signature> = HashMap::new();
    let mut extern_fn_ids: std::collections::HashSet<FuncId> =
        std::collections::HashSet::new();
    for (idx, func) in prog.functions.iter().enumerate() {
        let mid = FuncId(idx as u32);
        let sig = clif_signature_for(&module, func, prog)?;
        let linkage = if matches!(func.kind, ilang_mir::FunctionKind::Extern { .. }) {
            extern_fn_ids.insert(mid);
            Linkage::Import
        } else {
            Linkage::Local
        };
        // For `@extern(C) @symbol("foo") fn bar(...)`, declare under
        // the C-side name `foo` so dlsym resolves correctly while
        // ilang-side calls still go through this FuncId via `bar`.
        let symbol_name: &str = if let Some(c) = func.c_symbol {
            c.as_str()
        } else {
            func.name.as_str()
        };
        let cid = module.declare_function(symbol_name, linkage, &sig)?;
        fn_ids.insert(mid, cid);
        fn_sigs.insert(mid, sig);
    }

    // Define each fn body.
    let mut ctx = module.make_context();
    let mut fb_ctx = FunctionBuilderContext::new();
    for (idx, func) in prog.functions.iter().enumerate() {
        let mid = FuncId(idx as u32);
        // Extern fns are imports — no body to compile.
        if extern_fn_ids.contains(&mid) {
            continue;
        }
        let sig = fn_sigs.get(&mid).unwrap().clone();
        let cid = *fn_ids.get(&mid).unwrap();
        ctx.func = ClifFunc::with_name_signature(UserFuncName::user(0, cid.as_u32()), sig);

        // Lower into ctx.func; we need &mut module to declare imports
        // for Inst::Call. Drop the FunctionBuilder before the next
        // module operation so borrows don't overlap.
        {
            let mut fb = ClifFnBuilder::new(&mut ctx.func, &mut fb_ctx);
            let map_ids = MapIds {
                new: map_new_id,
                get: map_get_id,
                get_optional: map_get_optional_id,
                set: map_set_id,
                size: map_size_id,
                has: map_has_id,
                delete: map_delete_id,
                keys: map_keys_id,
                values: map_values_id,
            };
            let panic_aux = PanicAux {
                fn_id: panic_fn_id,
                drop_dispatch: drop_dispatch_id,
                release_obj: release_obj_id,
                retain_obj: retain_obj_id,
                release_closure: release_closure_id,
                retain_closure: retain_closure_id,
                release_array: release_array_id,
                retain_array: retain_array_id,
                release_optional: release_optional_id,
                retain_optional: retain_optional_id,
                release_tuple: release_tuple_id,
                retain_tuple: retain_tuple_id,
                release_map: release_map_id,
                retain_map: retain_map_id,
                map_set_obj_val: map_set_obj_val_id,
                map_set_print_kinds: map_set_print_kinds_id,
                print_map: print_map_id,
                class_name: class_name_id,
                mir_free: free_id,
                release_string: release_string_id,
                retain_string: retain_string_id,
                enum_unit_get: enum_unit_get_id,
                enum_alloc: enum_alloc_id,
                release_enum: release_enum_id,
                retain_enum: retain_enum_id,
                msg_div: panic_msg_div,
                msg_mod: panic_msg_mod,
                msg_oob: panic_msg_oob,
                msg_unwrap: panic_msg_unwrap,
            };
            let print_lits = PrintLits {
                none: lit_none,
                some_open: lit_some_open,
                close_paren: lit_close_paren,
                open_paren: lit_open_paren,
                open_bracket: lit_open_bracket,
                close_bracket: lit_close_bracket,
                comma_sp: lit_comma_sp,
            };
            lower_function(
                &mut fb,
                func,
                &fn_ids,
                &fn_sigs,
                &builtin_ids,
                &static_data,
                &string_data,
                alloc_id,
                map_ids,
                str_ids,
                print_ids,
                panic_aux,
                print_lits,
                &mut module,
                prog,
                &class_global,
                &enum_global,
            )?;
            fb.finalize();
        }

        if std::env::var("ILANG_DUMP_CLIF").is_ok() {
            eprintln!("=== {} clif ===\n{}", func.name.as_str(), ctx.func.display());
        }
        if let Err(e) = module.define_function(cid, &mut ctx) {
            return Err(CompileError::Other(format!(
                "define_function `{}`: {e:?}\nclif IR:\n{}",
                func.name,
                ctx.func.display()
            )));
        }
        module.clear_context(&mut ctx);
    }
    module
        .finalize_definitions()
        .map_err(CompileError::Module)?;

    // Populate the runtime vtable now that fn addresses are stable.
    // Don't clear — entries are keyed by GLOBAL (class_id, slot) and
    // accumulate so parallel modules coexist without trampling.
    {
        let mut vt = vtable_lock().lock().expect("vtable poisoned");
        for ((cid, slot), fid) in &vtable_entries {
            if let Some(cl_id) = fn_ids.get(fid) {
                let addr = module.get_finalized_function(*cl_id) as i64;
                vt.insert((*cid, *slot), addr);
            }
        }
    }
    // Populate Object field table — host_release_object_fields uses
    // it to cascade releases through heap-shaped fields.
    {
        // Scan the whole program for any MirTy::Weak(C) reference —
        // classes that appear as a weak target stay OUT of the size
        // table so release_object's free path skips them. Without
        // this, a `let w: Node.weak = strong; …; strong = …` flow
        // (see weak_basic.il) would have the weak peek into freed
        // memory once the original strong drops. The leak we accept
        // for those classes is bounded — programs that use weak
        // refs are usually small fixed graphs.
        let mut weakable: std::collections::HashSet<u32> =
            std::collections::HashSet::new();
        let scan = |ty: &MirTy, set: &mut std::collections::HashSet<u32>| {
            walk_mir_ty(ty, &mut |t| {
                if let MirTy::Weak(c) = t {
                    set.insert(c.0);
                }
            });
        };
        for class in &prog.classes {
            for f in &class.fields {
                scan(&f.ty, &mut weakable);
            }
        }
        for f in &prog.functions {
            for p in f.params.iter() {
                scan(&p.ty, &mut weakable);
            }
            scan(&f.ret, &mut weakable);
            for l in f.value_tys.iter() {
                scan(l, &mut weakable);
            }
            for l in f.local_tys.iter() {
                scan(l, &mut weakable);
            }
        }

        let mut t = object_field_table_lock()
            .lock()
            .expect("field table poisoned");
        let mut sizes = class_size_table_lock()
            .lock()
            .expect("class size table poisoned");
        // Don't clear — entries are keyed by GLOBAL class id and
        // accumulate across compiles so parallel modules coexist.
        for class in &prog.classes {
            let mut entries: Vec<(i64, PrintKind)> = Vec::new();
            for (i, f) in class.fields.iter().enumerate() {
                let kind = print_kind_of(&f.ty);
                let needs = matches!(
                    kind,
                    PrintKind::Object
                        | PrintKind::Optional(_)
                        | PrintKind::Array(_)
                        | PrintKind::Tuple(_)
                );
                if needs {
                    let off = OBJECT_HEADER_BYTES as i64 + (i as i64) * 8;
                    entries.push((off, kind));
                }
            }
            t.insert(global_cid(class.id.0), entries);
            // Mirror NewObject codegen: regular classes alloc
            // header + n_fields*8 bytes. Skip CRepr/packed/union
            // (different free path) and any class referenced via
            // Weak (would dangle weak peeks).
            let skip_free = matches!(
                class.repr,
                ilang_mir::ClassRepr::CRepr
                    | ilang_mir::ClassRepr::CPacked
                    | ilang_mir::ClassRepr::CUnion
            ) || weakable.contains(&class.id.0);
            if !skip_free {
                let size = OBJECT_HEADER_BYTES as i64 + (class.fields.len() as i64) * 8;
                sizes.insert(global_cid(class.id.0), size);
            }
        }
    }
    // Populate the class-print-info registry — host_print_object
    // walks an object's fields by class id.
    {
        let mut info_map = class_info_lock().lock().expect("class info poisoned");
        // Don't clear — keyed by GLOBAL class id, accumulates.
        for class in &prog.classes {
            let fields: Vec<(String, PrintKind)> = class
                .fields
                .iter()
                .map(|f| (f.name.as_str().to_string(), print_kind_of(&f.ty)))
                .collect();
            info_map.insert(
                global_cid(class.id.0),
                ClassPrintInfo {
                    name: class.name.as_str().to_string(),
                    fields,
                },
            );
        }
    }
    // Populate enum-print-info registry — host_print_enum walks
    // enum tag → variant name + payload kinds.
    {
        let mut t = enum_info_lock().lock().expect("enum info poisoned");
        // Don't clear — keyed by GLOBAL enum id, accumulates.
        for e in &prog.enums {
            let mut variants: HashMap<i64, (String, Vec<PrintKind>)> = HashMap::new();
            for v in &e.variants {
                let kinds: Vec<PrintKind> = match &v.payload {
                    ilang_mir::VariantPayload::Unit => Vec::new(),
                    ilang_mir::VariantPayload::Tuple(tys) => {
                        tys.iter().map(print_kind_of).collect()
                    }
                    ilang_mir::VariantPayload::Struct(fs) => {
                        fs.iter().map(|(_, t)| print_kind_of(t)).collect()
                    }
                };
                variants.insert(v.discriminant, (v.name.as_str().to_string(), kinds));
            }
            t.insert(
                global_eid(e.id.0),
                EnumPrintInfo {
                    name: e.name.as_str().to_string(),
                    variants,
                },
            );
        }
    }
    // Populate closure capture-info registry — host_release_closure
    // walks heap-shaped captures so they release on closure drop.
    {
        let mut t = closure_capture_table_lock()
            .lock()
            .expect("closure capture table poisoned");
        let mut sizes = closure_size_table_lock()
            .lock()
            .expect("closure size table poisoned");
        // Don't clear — entries are keyed by fn_addr (globally
        // unique runtime address) and accumulate so parallel
        // modules coexist.
        for (idx, func) in prog.functions.iter().enumerate() {
            if extern_fn_ids.contains(&FuncId(idx as u32)) {
                continue;
            }
            let env = match &func.closure_env {
                Some(e) => e,
                None => continue,
            };
            let mid = FuncId(idx as u32);
            let cl_id = match fn_ids.get(&mid) {
                Some(c) => *c,
                None => continue,
            };
            let addr = module.get_finalized_function(cl_id) as i64;
            let mut entries: Vec<(i64, PrintKind)> = Vec::new();
            for (i, cap) in env.captures.iter().enumerate() {
                if cap.is_cell {
                    // Cells are 1-element arrays — leak for now.
                    continue;
                }
                let kind = print_kind_of(&cap.ty);
                let needs = matches!(
                    kind,
                    PrintKind::Object
                        | PrintKind::Optional(_)
                        | PrintKind::Array(_)
                        | PrintKind::Tuple(_)
                );
                if needs {
                    let off = 16 + (i as i64) * 8;
                    entries.push((off, kind));
                }
            }
            t.insert(addr, entries);
            // Mirror the MakeClosure codegen layout:
            // [fn_addr @ 0 | rc @ 8 | capture_0 @ 16 | …] = (2 + n_caps)*8.
            let total_size = (2 + env.captures.len() as i64) * 8;
            sizes.insert(addr, total_size);
        }
    }
    // Populate fn-name registry — host_print_fn looks up the fn
    // address (closure[0]) and prints "<fn NAME>" / "<fn>". Skip
    // extern fns (no compiled body) and synthetic names.
    {
        let mut m = fn_name_lock().lock().expect("fn name table poisoned");
        m.clear();
        for (idx, func) in prog.functions.iter().enumerate() {
            if extern_fn_ids.contains(&FuncId(idx as u32)) {
                continue;
            }
            let mid = FuncId(idx as u32);
            if let Some(cl_id) = fn_ids.get(&mid) {
                let addr = module.get_finalized_function(*cl_id) as i64;
                let name = func.name.as_str();
                if !name.starts_with("__anon_fn_") && !name.starts_with("__main") {
                    let plain = name.split("__").next().unwrap_or(name);
                    m.insert(addr, plain.to_string());
                }
            }
        }
    }
    // Populate the class-id → drop fn registry. `drop_fn` is set by
    // the lowering whenever a class declares `deinit`. Subclasses
    // that don't redefine deinit inherit the parent's via the
    // method table — read it back from the lowered class.
    {
        let mut dt = drop_table_lock().lock().expect("drop table poisoned");
        // Don't clear — keyed by GLOBAL class id, accumulates.
        for class in &prog.classes {
            if class.drop_fn.0 != u32::MAX {
                if let Some(cl_id) = fn_ids.get(&class.drop_fn) {
                    let addr = module.get_finalized_function(*cl_id) as i64;
                    dt.insert(global_cid(class.id.0), addr);
                }
            }
        }
    }

    let entry_fn = &prog.functions[prog.entry.0 as usize];
    let entry_ret = entry_fn.ret.clone();
    let entry = *fn_ids.get(&prog.entry).expect("entry registered");

    Ok(Compiled { module, entry, entry_ret })
}

/// Run the program's entry fn (assumed to be `() -> i64`) and return
/// the integer return value.
pub fn run_main(c: &Compiled) -> i64 {
    let ptr = c.module.get_finalized_function(c.entry);
    let f: extern "C" fn() -> i64 = unsafe { std::mem::transmute(ptr) };
    f()
}

/// Emit a boolean expression equivalent to `class_id ∈ {target ∪ all
/// transitive subclasses of target}`. Implements `is_instance` /
/// downcast eligibility for the language's single-inheritance model.
fn emit_is_subclass(
    fb: &mut ClifFnBuilder,
    class_id_value: Value,
    target: ClassId,
    prog: &Program,
    class_global: &[u32],
) -> Value {
    // Collect target + every descendant via a single hierarchy scan.
    // Local class ids first; we translate the final accept set to
    // GLOBAL ids before emitting the icmp chain (the runtime cid
    // loaded from obj+0 is the GLOBAL id).
    let mut accept: Vec<u32> = vec![target.0];
    loop {
        let before = accept.len();
        for c in &prog.classes {
            if let Some(p) = c.parent {
                if !accept.contains(&c.id.0) && accept.contains(&p.0) {
                    accept.push(c.id.0);
                }
            }
        }
        if accept.len() == before {
            break;
        }
    }
    let mut result: Option<Value> = None;
    for local in accept {
        let global = class_global[local as usize] as i64;
        let lit = fb.ins().iconst(types::I64, global);
        let eq = fb.ins().icmp(IntCC::Equal, class_id_value, lit);
        result = Some(match result {
            Some(prev) => fb.ins().bor(prev, eq),
            None => eq,
        });
    }
    result.unwrap_or_else(|| fb.ins().iconst(types::I8, 0))
}

/// Bring a clif value up to i64 by sign/zero-extension or bitcast.
/// Used when storing a primitive into an i64-cell-shaped slot
/// (object field, array cell, static slot, optional payload).
fn extend_to_i64(fb: &mut ClifFnBuilder, v: Value) -> Value {
    let ty = fb.func.dfg.value_type(v);
    if ty == types::I64 {
        v
    } else if ty == types::F64 {
        fb.ins().bitcast(types::I64, MemFlags::new(), v)
    } else if ty == types::F32 {
        let r32 = fb.ins().bitcast(types::I32, MemFlags::new(), v);
        fb.ins().uextend(types::I64, r32)
    } else {
        fb.ins().uextend(types::I64, v)
    }
}

/// Inverse of `extend_to_i64`: take an i64-cell value and produce
/// the right-sized clif value for `target_ty`.
fn reduce_from_i64(fb: &mut ClifFnBuilder, target_ty: &MirTy, raw: Value) -> Value {
    match target_ty {
        MirTy::I64 | MirTy::U64 | MirTy::Size | MirTy::SSize => raw,
        MirTy::I32 | MirTy::U32 => fb.ins().ireduce(types::I32, raw),
        MirTy::I16 | MirTy::U16 => fb.ins().ireduce(types::I16, raw),
        MirTy::I8 | MirTy::U8 | MirTy::Bool | MirTy::CChar => fb.ins().ireduce(types::I8, raw),
        MirTy::F64 => fb.ins().bitcast(types::F64, MemFlags::new(), raw),
        MirTy::F32 => {
            let r32 = fb.ins().ireduce(types::I32, raw);
            fb.ins().bitcast(types::F32, MemFlags::new(), r32)
        }
        _ => raw,
    }
}

use std::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};
use std::sync::Mutex;
use std::sync::OnceLock;

/// Globally-unique class / enum id allocators. Each `compile_program`
/// call carves out a fresh range so the per-program local id space
/// (`class.id.0`, `enum.id.0`) never collides with a parallel
/// compilation's. NewObject / EnumCtor codegen stores the GLOBAL id
/// into the heap header, and every host-side table (vtable,
/// object_field_table, drop_table, class_info, enum_info) is keyed
/// by that same global id. Without this, parallel cargo-test
/// processes corrupted each other's tables and SIGSEGV'd inside
/// release_object on the first deinit.
static NEXT_GLOBAL_CLASS_ID: AtomicU32 = AtomicU32::new(1);
static NEXT_GLOBAL_ENUM_ID: AtomicU32 = AtomicU32::new(1);

fn alloc_global_class_id() -> u32 {
    NEXT_GLOBAL_CLASS_ID.fetch_add(1, AtomicOrdering::Relaxed)
}

fn alloc_global_enum_id() -> u32 {
    NEXT_GLOBAL_ENUM_ID.fetch_add(1, AtomicOrdering::Relaxed)
}

/// Runtime vtable: (global_class_id, slot) → fn pointer (i64).
/// Persistent across compiles — entries accumulate so multiple
/// independently-compiled modules can coexist without trampling.
static VTABLE: OnceLock<Mutex<HashMap<(u32, u32), i64>>> = OnceLock::new();

fn vtable_lock() -> &'static Mutex<HashMap<(u32, u32), i64>> {
    VTABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

extern "C" fn host_virt_dispatch(class_id: i64, slot: i64) -> i64 {
    let m = vtable_lock().lock().expect("vtable poisoned");
    *m.get(&(class_id as u32, slot as u32)).unwrap_or(&0)
}

static DROP_TABLE: OnceLock<Mutex<HashMap<u32, i64>>> = OnceLock::new();

fn drop_table_lock() -> &'static Mutex<HashMap<u32, i64>> {
    DROP_TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

extern "C" fn host_drop_dispatch(class_id: i64) -> i64 {
    let m = drop_table_lock().lock().expect("drop table poisoned");
    *m.get(&(class_id as u32)).unwrap_or(&0)
}

/// Per-class registry: list of (field_offset, FieldKind) for fields
/// whose static type is heap-shaped (Object / Array / Optional / etc).
/// On Release, after the user's deinit fires, we walk this list and
/// release each — gives us the cascade without per-class generated
/// drop fns.
static OBJECT_FIELD_TABLE: OnceLock<Mutex<HashMap<u32, Vec<(i64, PrintKind)>>>> =
    OnceLock::new();

fn object_field_table_lock() -> &'static Mutex<HashMap<u32, Vec<(i64, PrintKind)>>> {
    OBJECT_FIELD_TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// global_class_id → total byte size of the heap allocation. Used by
/// `release_object` to free obj_ptr once rc reaches 0. CRepr classes
/// stay out of this table — their lifetime is tracked through the
/// codegen-side `__mir_free(ptr, c_size)` emit.
static CLASS_SIZE_TABLE: OnceLock<Mutex<HashMap<u32, i64>>> = OnceLock::new();

fn class_size_table_lock() -> &'static Mutex<HashMap<u32, i64>> {
    CLASS_SIZE_TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Visit every MirTy reachable from `ty` (recursing through Array
/// element / Optional inner / Tuple components / Map key+value /
/// Fn params+ret / RawPtr inner). Used by the weakable-class scan.
fn walk_mir_ty(ty: &MirTy, f: &mut impl FnMut(&MirTy)) {
    f(ty);
    match ty {
        MirTy::Array { elem, .. } => walk_mir_ty(elem, f),
        MirTy::Optional(inner) => walk_mir_ty(inner, f),
        MirTy::Tuple(items) => {
            for t in items.iter() {
                walk_mir_ty(t, f);
            }
        }
        MirTy::Map { key, val } => {
            walk_mir_ty(key, f);
            walk_mir_ty(val, f);
        }
        MirTy::Fn(ft) => {
            for p in ft.params.iter() {
                walk_mir_ty(p, f);
            }
            walk_mir_ty(&ft.ret, f);
        }
        MirTy::RawPtr { inner, .. } => walk_mir_ty(inner, f),
        _ => {}
    }
}

extern "C" fn host_release_object(obj_ptr: i64) {
    release_object(obj_ptr);
}

/// fn_addr → list of (capture_offset, kind) for heap-shaped captures
/// that need release on closure drop.
static CLOSURE_CAPTURE_TABLE: OnceLock<Mutex<HashMap<i64, Vec<(i64, PrintKind)>>>> =
    OnceLock::new();

fn closure_capture_table_lock() -> &'static Mutex<HashMap<i64, Vec<(i64, PrintKind)>>> {
    CLOSURE_CAPTURE_TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// fn_addr → total byte size of the closure heap cell. Used by
/// `host_release_closure` to free the block once rc reaches 0.
static CLOSURE_SIZE_TABLE: OnceLock<Mutex<HashMap<i64, i64>>> = OnceLock::new();

fn closure_size_table_lock() -> &'static Mutex<HashMap<i64, i64>> {
    CLOSURE_SIZE_TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

extern "C" fn host_release_closure(closure_ptr: i64) {
    if closure_ptr == 0 {
        return;
    }
    let rc_ptr = (closure_ptr + 8) as *mut i64;
    let rc = unsafe { *rc_ptr };
    if rc <= 0 {
        return;
    }
    let new_rc = rc - 1;
    unsafe {
        *rc_ptr = new_rc;
    }
    if new_rc != 0 {
        return;
    }
    let fn_addr = unsafe { *(closure_ptr as *const i64) };
    let entries = {
        let m = closure_capture_table_lock()
            .lock()
            .expect("closure capture table poisoned");
        m.get(&fn_addr).cloned()
    };
    if let Some(entries) = entries {
        for (off, kind) in entries.iter() {
            let raw = unsafe { *((closure_ptr + *off) as *const i64) };
            release_value_by_kind(raw, kind);
        }
    }
    // Free the closure cell. Size is keyed off fn_addr — registered
    // at compile time alongside the capture table.
    let size = {
        let m = closure_size_table_lock()
            .lock()
            .expect("closure size table poisoned");
        m.get(&fn_addr).copied()
    };
    if let Some(size) = size {
        host_mir_free(closure_ptr, size);
    }
}

extern "C" fn host_retain_closure(closure_ptr: i64) {
    if closure_ptr == 0 {
        return;
    }
    let rc_ptr = (closure_ptr + 8) as *mut i64;
    let rc = unsafe { *rc_ptr };
    if rc <= 0 {
        return;
    }
    unsafe {
        *rc_ptr = rc + 1;
    }
}

extern "C" fn host_release_tuple(tup_ptr: i64) {
    if tup_ptr == 0 {
        return;
    }
    let base = tup_ptr - 16;
    let rc_ptr = base as *mut i64;
    let rc = unsafe { *rc_ptr };
    if rc <= 0 {
        return;
    }
    let new_rc = rc - 1;
    unsafe {
        *rc_ptr = new_rc;
    }
    if new_rc != 0 {
        return;
    }
    let packed = unsafe { *((base + 8) as *const i64) } as u64;
    // packed layout (set by NewTuple codegen):
    //   bits  0-15 : arity (max 65535 elements)
    //   bits 16-63 : 4-bit KIND_* tag per element, up to 12
    //                elements (12 × 4 = 48 bits). Tuples > 12
    //                elements have their kinds 12+ implicitly
    //                KIND_NONE — those slots leak heap content
    //                but the cell itself is still freed.
    let arity = (packed & 0xFFFF) as i64;
    for i in 0..arity.min(12) {
        let kind = ((packed >> (16 + (i as u64) * 4)) & 0xF) as i64;
        if kind != KIND_NONE {
            let elem = unsafe { *((tup_ptr + i * 8) as *const i64) };
            release_by_kind(elem, kind);
        }
    }
    // Free the tuple cell. base = tup_ptr - 16; total = 16 + arity*8.
    host_mir_free(base, 16 + arity.max(1) * 8);
}

extern "C" fn host_retain_tuple(tup_ptr: i64) {
    if tup_ptr == 0 {
        return;
    }
    let rc_ptr = (tup_ptr - 16) as *mut i64;
    let rc = unsafe { *rc_ptr };
    if rc <= 0 {
        return;
    }
    unsafe {
        *rc_ptr = rc + 1;
    }
}

extern "C" fn host_release_optional(opt_ptr: i64) {
    if opt_ptr == 0 {
        return;
    }
    let rc_ptr = (opt_ptr + 8) as *mut i64;
    let rc = unsafe { *rc_ptr };
    if rc <= 0 {
        return;
    }
    let new_rc = rc - 1;
    unsafe {
        *rc_ptr = new_rc;
    }
    if new_rc != 0 {
        return;
    }
    let tag = unsafe { *((opt_ptr + 16) as *const i64) };
    if tag != KIND_NONE {
        let inner = unsafe { *(opt_ptr as *const i64) };
        release_by_kind(inner, tag);
    }
    // Free the 24-byte Optional cell. The earlier some(_) over-
    // release concern was resolved by 192b91d (Some/Break/EnumCtor
    // retain on aliased heap inner) so freeing the cell on rc=0
    // is now safe — fresh `some(new T())` transfers the inner's
    // +1 to the Optional, aliased `some(x)` bumps rc on the inner.
    host_mir_free(opt_ptr, 24);
}

extern "C" fn host_retain_optional(opt_ptr: i64) {
    if opt_ptr == 0 {
        return;
    }
    let rc_ptr = (opt_ptr + 8) as *mut i64;
    let rc = unsafe { *rc_ptr };
    if rc <= 0 {
        return;
    }
    unsafe {
        *rc_ptr = rc + 1;
    }
}

extern "C" fn host_release_array(arr_ptr: i64) {
    if arr_ptr == 0 {
        return;
    }
    let rc_ptr = (arr_ptr + 24) as *mut i64;
    let rc = unsafe { *rc_ptr };
    if rc <= 0 {
        return;
    }
    let new_rc = rc - 1;
    unsafe {
        *rc_ptr = new_rc;
    }
    if new_rc != 0 {
        return;
    }
    let tag = unsafe { *((arr_ptr + 32) as *const i64) };
    let len = unsafe { *(arr_ptr as *const i64) };
    let cap = unsafe { *((arr_ptr + 8) as *const i64) };
    let data_ptr = unsafe { *((arr_ptr + 16) as *const i64) };
    let stride = unsafe { *((arr_ptr + 40) as *const i64) };
    // Cascade-release each stored element by its kind. The tag was
    // 0/1 ("Object or not") under the old scheme; it now carries
    // the full KIND_* discriminant so Array<Array<...>> /
    // Array<Optional<...>> / Array<Str> reclaim their inner cells
    // too. KIND_NONE skips the loop (primitive elements).
    if tag != KIND_NONE {
        for i in 0..len {
            let elem = unsafe { *((data_ptr + i * 8) as *const i64) };
            release_by_kind(elem, tag);
        }
    }
    // Free the data buffer + the 48-byte header. Both came from
    // host_mir_alloc in NewArray / NewArrayEmpty / build_array /
    // host_array_push grow path, so reconstructing the same byte
    // counts via host_mir_free drops the underlying Vec.
    if data_ptr != 0 {
        host_mir_free(data_ptr, cap.max(1) * stride);
    }
    host_mir_free(arr_ptr, 48);
}

extern "C" fn host_retain_array(arr_ptr: i64) {
    if arr_ptr == 0 {
        return;
    }
    let rc_ptr = (arr_ptr + 24) as *mut i64;
    let rc = unsafe { *rc_ptr };
    if rc <= 0 {
        return;
    }
    unsafe {
        *rc_ptr = rc + 1;
    }
}

extern "C" fn host_retain_object(obj_ptr: i64) {
    if obj_ptr == 0 {
        return;
    }
    let rc_ptr = (obj_ptr + 8) as *mut i64;
    let rc = unsafe { *rc_ptr };
    if rc <= 0 {
        return;
    }
    unsafe {
        *rc_ptr = rc + 1;
    }
}

extern "C" fn host_release_object_fields(class_id: i64, obj_ptr: i64) {
    if obj_ptr == 0 {
        return;
    }
    let entries = {
        let m = object_field_table_lock()
            .lock()
            .expect("field table poisoned");
        m.get(&(class_id as u32)).cloned()
    };
    let entries = match entries {
        Some(e) if !e.is_empty() => e,
        _ => return,
    };
    for (off, kind) in entries.iter() {
        let raw = unsafe { *((obj_ptr + *off) as *const i64) };
        release_value_by_kind(raw, kind);
    }
}

fn release_value_by_kind(raw: i64, kind: &PrintKind) {
    match kind {
        PrintKind::Object => {
            release_object(raw);
        }
        PrintKind::Optional(inner) => {
            if raw != 0 {
                let payload = unsafe { *(raw as *const i64) };
                release_value_by_kind(payload, inner);
            }
        }
        PrintKind::Array(inner) => {
            if raw != 0
                && matches!(
                    **inner,
                    PrintKind::Object
                        | PrintKind::Optional(_)
                        | PrintKind::Array(_)
                        | PrintKind::Tuple(_)
                )
            {
                let len = unsafe { *(raw as *const i64) };
                let data_ptr = unsafe { *((raw + 16) as *const i64) };
                for i in 0..len {
                    let elem_raw =
                        unsafe { *((data_ptr + (i * 8)) as *const i64) };
                    release_value_by_kind(elem_raw, inner);
                }
            }
        }
        PrintKind::Tuple(items) => {
            if raw != 0 {
                for (i, k) in items.iter().enumerate() {
                    let elem_raw =
                        unsafe { *((raw + (i as i64) * 8) as *const i64) };
                    release_value_by_kind(elem_raw, k);
                }
            }
        }
        _ => {}
    }
}

// Element / inner-value kind tags used by Array, Optional and (in
// the future) Tuple cells to dispatch the right release function
// at cascade time. NewArray / NewOptional codegen writes these
// into the heap header; `release_by_kind` below reads back.
const KIND_NONE: i64 = 0;
const KIND_OBJECT: i64 = 1;
const KIND_ARRAY: i64 = 2;
const KIND_OPTIONAL: i64 = 3;
const KIND_TUPLE: i64 = 4;
const KIND_MAP: i64 = 5;
const KIND_CLOSURE: i64 = 6;
const KIND_STR: i64 = 7;
const KIND_ENUM: i64 = 8;

/// Compute the kind tag for a static MirTy. Used at compile time
/// when a heap container (Array / Optional) emits its header.
fn kind_tag_of(ty: &MirTy) -> i64 {
    match ty {
        MirTy::Object(_) => KIND_OBJECT,
        MirTy::Array { .. } => KIND_ARRAY,
        MirTy::Optional(_) => KIND_OPTIONAL,
        MirTy::Tuple(_) => KIND_TUPLE,
        MirTy::Map { .. } => KIND_MAP,
        MirTy::Fn(_) => KIND_CLOSURE,
        MirTy::Str => KIND_STR,
        MirTy::Enum(_) => KIND_ENUM,
        _ => KIND_NONE,
    }
}

/// Dispatch release on a runtime value given its static kind.
/// Recurses through nested containers (Array of Array, Optional
/// of Array, etc.) so deep cascades reclaim every level.
fn release_by_kind(ptr: i64, kind: i64) {
    if ptr == 0 {
        return;
    }
    match kind {
        KIND_OBJECT => release_object(ptr),
        KIND_ARRAY => host_release_array(ptr),
        KIND_OPTIONAL => host_release_optional(ptr),
        KIND_TUPLE => host_release_tuple(ptr),
        KIND_MAP => host_release_map(ptr),
        KIND_CLOSURE => host_release_closure(ptr),
        KIND_STR => host_release_string(ptr),
        KIND_ENUM => host_release_enum(ptr),
        _ => {} // KIND_NONE / unknown — primitive, no cascade.
    }
}

/// Translate a `PrintKind` (used by enum_info / object_field_table)
/// to the runtime KIND_* tag, then dispatch through
/// `release_by_kind`. Lets enum-payload cascade reuse the new
/// rc-aware release path instead of the older in-line walker
/// that didn't decrement intermediate cells' rc.
fn release_print_kind(raw: i64, kind: &PrintKind) {
    let tag = match kind {
        PrintKind::Object => KIND_OBJECT,
        PrintKind::Array(_) => KIND_ARRAY,
        PrintKind::Optional(_) => KIND_OPTIONAL,
        PrintKind::Tuple(_) => KIND_TUPLE,
        PrintKind::Str => KIND_STR,
        _ => KIND_NONE,
    };
    release_by_kind(raw, tag);
}

/// Mirror of `release_by_kind` for retain. Used when one container
/// hands an element pointer to another (e.g. `arr.filter(...)`
/// keeps a subset of the source array's elements; both arrays then
/// own the kept elements at +1 each).
fn retain_by_kind(ptr: i64, kind: i64) {
    if ptr == 0 {
        return;
    }
    match kind {
        KIND_OBJECT => host_retain_object(ptr),
        KIND_ARRAY => host_retain_array(ptr),
        KIND_OPTIONAL => host_retain_optional(ptr),
        KIND_TUPLE => host_retain_tuple(ptr),
        KIND_MAP => host_retain_map(ptr),
        KIND_CLOSURE => host_retain_closure(ptr),
        KIND_STR => host_retain_string(ptr),
        KIND_ENUM => host_retain_enum(ptr),
        _ => {}
    }
}

fn release_object(obj_ptr: i64) {
    if obj_ptr == 0 {
        return;
    }
    let rc_ptr = (obj_ptr + 8) as *mut i64;
    let rc = unsafe { *rc_ptr };
    if rc <= 0 {
        return;
    }
    let new_rc = rc - 1;
    unsafe {
        *rc_ptr = new_rc;
    }
    if new_rc != 0 {
        return;
    }
    let class_id = unsafe { *(obj_ptr as *const i64) };
    // Call user deinit if registered.
    let user_drop = {
        let m = drop_table_lock().lock().expect("drop table poisoned");
        m.get(&(class_id as u32)).copied().unwrap_or(0)
    };
    if user_drop != 0 {
        let f: extern "C" fn(i64, i64) = unsafe { std::mem::transmute(user_drop) };
        f(obj_ptr, 0);
    }
    host_release_object_fields(class_id, obj_ptr);
    // Object cell free is staged (class_size_table is populated)
    // but currently gated. Even with the super-init retain fix,
    // a few patterns still mis-account: closure-capture escape
    // (a captured local releases at maker-fn scope exit before
    // the closure escapes), and Optional<Object[]> sequences
    // ordering that combines unwrap-extracted aliases with later
    // reuse. Until those callees retain the borrow, leaving the
    // free disabled trades extra residency for safety.
    let size = {
        let m = class_size_table_lock()
            .lock()
            .expect("class size table poisoned");
        m.get(&(class_id as u32)).copied()
    };
    let _ = size;
}

#[derive(Clone)]
enum PrintKind {
    I64Sig,
    I64Uns,
    I32Sig,
    I32Uns,
    I16Sig,
    I16Uns,
    I8Sig,
    I8Uns,
    Bool,
    F64,
    F32,
    Str,
    Object,
    Array(Box<PrintKind>),
    Optional(Box<PrintKind>),
    Tuple(Vec<PrintKind>),
    Other,
}

#[derive(Clone)]
struct ClassPrintInfo {
    name: String,
    fields: Vec<(String, PrintKind)>,
}

static CLASS_INFO: OnceLock<Mutex<HashMap<u32, ClassPrintInfo>>> = OnceLock::new();

fn class_info_lock() -> &'static Mutex<HashMap<u32, ClassPrintInfo>> {
    CLASS_INFO.get_or_init(|| Mutex::new(HashMap::new()))
}

fn print_kind_of(ty: &MirTy) -> PrintKind {
    match ty {
        MirTy::Bool => PrintKind::Bool,
        MirTy::I64 => PrintKind::I64Sig,
        MirTy::U64 => PrintKind::I64Uns,
        MirTy::I32 => PrintKind::I32Sig,
        MirTy::U32 => PrintKind::I32Uns,
        MirTy::I16 => PrintKind::I16Sig,
        MirTy::U16 => PrintKind::I16Uns,
        MirTy::I8 => PrintKind::I8Sig,
        MirTy::U8 => PrintKind::I8Uns,
        MirTy::F64 => PrintKind::F64,
        MirTy::F32 => PrintKind::F32,
        MirTy::Str => PrintKind::Str,
        MirTy::Object(_) => PrintKind::Object,
        MirTy::Array { elem, .. } => PrintKind::Array(Box::new(print_kind_of(elem))),
        MirTy::Optional(inner) => PrintKind::Optional(Box::new(print_kind_of(inner))),
        MirTy::Tuple(items) => {
            PrintKind::Tuple(items.iter().map(print_kind_of).collect())
        }
        _ => PrintKind::Other,
    }
}

fn format_value(out: &mut String, kind: &PrintKind, raw: i64) {
    use std::fmt::Write;
    match kind {
        PrintKind::I64Sig => { let _ = write!(out, "{}", raw); }
        PrintKind::I64Uns => { let _ = write!(out, "{}", raw as u64); }
        PrintKind::I32Sig => { let _ = write!(out, "{}", raw as i32); }
        PrintKind::I32Uns => { let _ = write!(out, "{}", raw as u32); }
        PrintKind::I16Sig => { let _ = write!(out, "{}", raw as i16); }
        PrintKind::I16Uns => { let _ = write!(out, "{}", raw as u16); }
        PrintKind::I8Sig => { let _ = write!(out, "{}", raw as i8); }
        PrintKind::I8Uns => { let _ = write!(out, "{}", raw as u8); }
        PrintKind::Bool => { let _ = write!(out, "{}", raw != 0); }
        PrintKind::F64 => {
            let f = f64::from_bits(raw as u64);
            let _ = write!(out, "{}", format_f64(f));
        }
        PrintKind::F32 => {
            let f = f32::from_bits((raw as i32) as u32);
            let _ = write!(out, "{}", format_f64(f as f64));
        }
        PrintKind::Str => {
            if raw == 0 {
                let _ = write!(out, "");
            } else {
                let bytes = unsafe { cstr_bytes(raw) };
                let _ = write!(out, "{}", String::from_utf8_lossy(bytes));
            }
        }
        PrintKind::Object => {
            if raw == 0 {
                let _ = write!(out, "<null>");
            } else {
                format_object(out, raw);
            }
        }
        PrintKind::Array(inner) => {
            out.push('[');
            if raw != 0 {
                let len = unsafe { *(raw as *const i64) };
                let data_ptr = unsafe { *((raw + 16) as *const i64) };
                for i in 0..len {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    let elem_raw =
                        unsafe { *((data_ptr + (i * 8)) as *const i64) };
                    format_value(out, inner, elem_raw);
                }
            }
            out.push(']');
        }
        PrintKind::Optional(inner) => {
            if raw == 0 {
                out.push_str("none");
            } else {
                out.push_str("some(");
                let payload = unsafe { *(raw as *const i64) };
                format_value(out, inner, payload);
                out.push(')');
            }
        }
        PrintKind::Tuple(items) => {
            out.push('(');
            for (i, k) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                let elem_raw = unsafe { *((raw + (i as i64) * 8) as *const i64) };
                format_value(out, k, elem_raw);
            }
            out.push(')');
        }
        PrintKind::Other => {
            let _ = write!(out, "{}", raw);
        }
    }
}

fn format_f64(f: f64) -> String {
    if f.is_nan() {
        "NaN".to_string()
    } else if f.is_infinite() {
        if f > 0.0 { "Infinity".to_string() } else { "-Infinity".to_string() }
    } else if f == f.trunc() && f.abs() < 1e16 {
        format!("{}.0", f as i64)
    } else {
        format!("{}", f)
    }
}

fn format_object(out: &mut String, obj_ptr: i64) {
    let class_id = unsafe { *(obj_ptr as *const i64) } as u32;
    let info = {
        let m = class_info_lock().lock().expect("class info poisoned");
        m.get(&class_id).cloned()
    };
    let info = match info {
        Some(i) => i,
        None => {
            use std::fmt::Write;
            let _ = write!(out, "<obj#{class_id}>");
            return;
        }
    };
    // Strip the monomorphisation suffix (`Box<i64>` → `Box`) so the
    // printed name matches the source-level identifier.
    let base = info
        .name
        .split('<')
        .next()
        .unwrap_or(info.name.as_str());
    out.push_str(base);
    out.push_str(" {");
    if !info.fields.is_empty() {
        out.push(' ');
        for (i, (fname, fkind)) in info.fields.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(fname);
            out.push_str(": ");
            let raw = unsafe {
                *((obj_ptr + 16 + (i as i64) * 8) as *const i64)
            };
            format_value(out, fkind, raw);
        }
        out.push(' ');
    }
    out.push('}');
}

static FN_NAME_TABLE: OnceLock<Mutex<HashMap<i64, String>>> = OnceLock::new();

fn fn_name_lock() -> &'static Mutex<HashMap<i64, String>> {
    FN_NAME_TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

extern "C" fn host_print_fn(closure_ptr: i64) {
    if closure_ptr == 0 {
        print!("<fn>");
        return;
    }
    let fn_addr = unsafe { *(closure_ptr as *const i64) };
    let m = fn_name_lock().lock().expect("fn name table poisoned");
    if let Some(name) = m.get(&fn_addr) {
        print!("<fn {}>", name);
    } else {
        print!("<fn>");
    }
}

/// `typeof(x).name` resolves to this. Looks the class id up in the
/// per-class info registry and returns a leaked NUL-terminated copy
/// of the class name as a `*const u8`.
#[derive(Clone)]
struct EnumPrintInfo {
    name: String,
    /// discriminant → (variant_name, payload_kinds)
    variants: HashMap<i64, (String, Vec<PrintKind>)>,
}

static ENUM_INFO: OnceLock<Mutex<HashMap<u32, EnumPrintInfo>>> = OnceLock::new();

fn enum_info_lock() -> &'static Mutex<HashMap<u32, EnumPrintInfo>> {
    ENUM_INFO.get_or_init(|| Mutex::new(HashMap::new()))
}

extern "C" fn host_print_enum(enum_id: i64, ptr: i64) {
    let mut out = String::new();
    let info = {
        let m = enum_info_lock().lock().expect("enum info poisoned");
        m.get(&(enum_id as u32)).cloned()
    };
    let info = match info {
        Some(i) => i,
        None => {
            print!("<enum#{enum_id}>");
            return;
        }
    };
    if ptr == 0 {
        print!("{}::<null>", info.name);
        return;
    }
    let tag = unsafe { *(ptr as *const i64) };
    let (vname, pkinds) = match info.variants.get(&tag) {
        Some(v) => v.clone(),
        None => {
            print!("{}::<tag#{tag}>", info.name);
            return;
        }
    };
    // Strip generic suffix from enum name (Result<i64,string> → Result).
    let base = info.name.split('<').next().unwrap_or(info.name.as_str());
    out.push_str(base);
    out.push_str("::");
    out.push_str(&vname);
    if !pkinds.is_empty() {
        out.push('(');
        for (i, k) in pkinds.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            let raw = unsafe { *((ptr + 8 + (i as i64) * 8) as *const i64) };
            format_value(&mut out, k, raw);
        }
        out.push(')');
    }
    print!("{out}");
}

extern "C" fn host_print_weak(weak_ptr: i64) {
    if weak_ptr == 0 {
        print!("weak(<dead>)");
        return;
    }
    let rc = unsafe { *((weak_ptr + 8) as *const i64) };
    if rc <= 0 {
        print!("weak(<dead>)");
    } else {
        print!("weak(<alive>)");
    }
}

extern "C" fn host_class_name(class_id: i64) -> i64 {
    let info = {
        let m = class_info_lock().lock().expect("class info poisoned");
        m.get(&(class_id as u32)).cloned()
    };
    let name = match info {
        Some(i) => i.name,
        None => format!("<obj#{class_id}>"),
    };
    let base = name.split('<').next().unwrap_or(&name).to_string();
    leak_cstring(base)
}

extern "C" fn host_print_object(obj_ptr: i64) {
    let mut s = String::new();
    if obj_ptr == 0 {
        s.push_str("<null>");
    } else {
        format_object(&mut s, obj_ptr);
    }
    print!("{s}");
}

// Always-on counters for the host_mir_alloc / host_mir_free pair.
// Two atomic ops per alloc/free is a noise-floor cost for the JIT
// (~ns scale), and exposing the live deltas via `test.liveAlloc*`
// lets fixtures detect leaks of Object / Array / Optional / Tuple /
// Closure / internal cell allocations from inside an .il program.
// Strings (Box::leak) and ManagedMap (Rust HashMap) are tracked
// separately — see STRING_REGISTRY and the test.liveStringCount
// helper.
static ALLOC_BYTES: std::sync::atomic::AtomicI64 =
    std::sync::atomic::AtomicI64::new(0);
static FREE_BYTES: std::sync::atomic::AtomicI64 =
    std::sync::atomic::AtomicI64::new(0);
static ALLOC_COUNT: std::sync::atomic::AtomicI64 =
    std::sync::atomic::AtomicI64::new(0);
static FREE_COUNT: std::sync::atomic::AtomicI64 =
    std::sync::atomic::AtomicI64::new(0);

extern "C" fn host_mir_alloc(size: i64) -> i64 {
    let n = size as usize;
    let mut v: Vec<u8> = vec![0; n];
    let ptr = v.as_mut_ptr() as i64;
    std::mem::forget(v);
    ALLOC_BYTES.fetch_add(size, AtomicOrdering::Relaxed);
    ALLOC_COUNT.fetch_add(1, AtomicOrdering::Relaxed);
    ptr
}

/// Free a previously `host_mir_alloc`'d block. The caller must
/// know the exact `size` it was originally allocated with —
/// `host_mir_alloc` is just a leaked `Vec<u8>` and we reconstruct
/// the same vec to drop it. Used by the codegen's CRepr-struct
/// scope-exit release path so transient stack-replacement structs
/// (e.g. an SDL `Rect` built per draw call) actually free.
extern "C" fn host_mir_free(ptr: i64, size: i64) {
    if ptr == 0 || size <= 0 {
        return;
    }
    unsafe {
        let _ = Vec::from_raw_parts(ptr as *mut u8, size as usize, size as usize);
    }
    FREE_BYTES.fetch_add(size, AtomicOrdering::Relaxed);
    FREE_COUNT.fetch_add(1, AtomicOrdering::Relaxed);
}

/// `test.liveAllocBytes(): i64` — currently-held bytes through the
/// host_mir_alloc tracker. Leaks of Object / Array / Optional / etc.
/// show up as a steady delta after a release sweep.
extern "C" fn host_test_live_alloc_bytes() -> i64 {
    ALLOC_BYTES.load(AtomicOrdering::Relaxed)
        - FREE_BYTES.load(AtomicOrdering::Relaxed)
}

/// `test.liveAllocCount(): i64` — currently-held alloc count. Useful
/// for catching per-call cell allocations that should be cached or
/// released (e.g. unit-variant enum cells).
extern "C" fn host_test_live_alloc_count() -> i64 {
    ALLOC_COUNT.load(AtomicOrdering::Relaxed)
        - FREE_COUNT.load(AtomicOrdering::Relaxed)
}

/// `test.liveStringCount(): i64` — number of entries currently in
/// the rc-tracked string registry (leak_cstring buffers). Catches
/// `intToStr` / `str_concat` / `getError` etc. temps that should
/// have been released after their consumer ran.
extern "C" fn host_test_live_string_count() -> i64 {
    let reg = string_registry_lock()
        .lock()
        .expect("string registry poisoned");
    reg.len() as i64
}

/// Map runtime: a Rust HashMap<i64, i64> wrapped with rc + per-value
/// kind tag (1 = values are Object refs, cascade-release on drop).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum MapKey {
    Int(i64),
    Str(String),
}

struct ManagedMap {
    rc: i64,
    val_kind: i64,
    key_print_kind: i64,
    val_print_kind: i64,
    inner: std::collections::HashMap<MapKey, i64>,
    /// For string-keyed maps: canonical key → original C-string ptr
    /// the user inserted. Lets `keys()` return the original ptrs so
    /// downstream `arr.includes(orig)` works without content compare.
    str_key_origs: std::collections::HashMap<MapKey, i64>,
}

fn raw_to_map_key(raw: i64, key_print_kind: i64) -> MapKey {
    if key_print_kind == PK_STR {
        if raw == 0 {
            MapKey::Str(String::new())
        } else {
            let bytes = unsafe { cstr_bytes(raw) };
            MapKey::Str(String::from_utf8_lossy(bytes).into_owned())
        }
    } else {
        MapKey::Int(raw)
    }
}

fn map_key_to_raw(k: &MapKey) -> i64 {
    match k {
        MapKey::Int(n) => *n,
        MapKey::Str(s) => {
            // Re-emit a leaked length-prefixed copy so callers reading
            // back keys via host_map_keys see a stable ilang-string
            // pointer (matches the `[ i64 len | bytes | \0 ]` layout
            // used everywhere else).
            leak_cstring(s.clone())
        }
    }
}

const PK_I64_SIG: i64 = 0;
const PK_I64_UNS: i64 = 1;
const PK_I32_SIG: i64 = 2;
const PK_I32_UNS: i64 = 3;
const PK_I16_SIG: i64 = 4;
const PK_I16_UNS: i64 = 5;
const PK_I8_SIG: i64 = 6;
const PK_I8_UNS: i64 = 7;
const PK_BOOL: i64 = 8;
const PK_F64: i64 = 9;
const PK_F32: i64 = 10;
const PK_STR: i64 = 11;
const PK_OBJECT: i64 = 12;
const PK_ARRAY_I64_SIG: i64 = 100;
const PK_OTHER: i64 = -1;

fn print_kind_id(ty: &MirTy) -> i64 {
    match ty {
        MirTy::I64 | MirTy::Size | MirTy::SSize => PK_I64_SIG,
        MirTy::U64 => PK_I64_UNS,
        MirTy::I32 => PK_I32_SIG,
        MirTy::U32 => PK_I32_UNS,
        MirTy::I16 => PK_I16_SIG,
        MirTy::U16 => PK_I16_UNS,
        MirTy::I8 | MirTy::CChar => PK_I8_SIG,
        MirTy::U8 => PK_I8_UNS,
        MirTy::Bool => PK_BOOL,
        MirTy::F64 => PK_F64,
        MirTy::F32 => PK_F32,
        MirTy::Str => PK_STR,
        MirTy::Object(_) => PK_OBJECT,
        MirTy::Array { elem, .. } if matches!(**elem, MirTy::I64) => PK_ARRAY_I64_SIG,
        _ => PK_OTHER,
    }
}

fn format_kind_id(out: &mut String, kind: i64, raw: i64) {
    use std::fmt::Write;
    match kind {
        PK_I64_SIG => { let _ = write!(out, "{}", raw); }
        PK_I64_UNS => { let _ = write!(out, "{}", raw as u64); }
        PK_I32_SIG => { let _ = write!(out, "{}", raw as i32); }
        PK_I32_UNS => { let _ = write!(out, "{}", raw as u32); }
        PK_I16_SIG => { let _ = write!(out, "{}", raw as i16); }
        PK_I16_UNS => { let _ = write!(out, "{}", raw as u16); }
        PK_I8_SIG => { let _ = write!(out, "{}", raw as i8); }
        PK_I8_UNS => { let _ = write!(out, "{}", raw as u8); }
        PK_BOOL => { let _ = write!(out, "{}", raw != 0); }
        PK_F64 => {
            let f = f64::from_bits(raw as u64);
            let _ = write!(out, "{}", format_f64(f));
        }
        PK_F32 => {
            let f = f32::from_bits((raw as i32) as u32);
            let _ = write!(out, "{}", format_f64(f as f64));
        }
        PK_STR => {
            if raw != 0 {
                let bytes = unsafe { cstr_bytes(raw) };
                let _ = write!(out, "{}", String::from_utf8_lossy(bytes));
            }
        }
        PK_OBJECT => {
            if raw == 0 {
                out.push_str("<null>");
            } else {
                format_object(out, raw);
            }
        }
        PK_ARRAY_I64_SIG => {
            out.push('[');
            if raw != 0 {
                let len = unsafe { *(raw as *const i64) };
                let data_ptr = unsafe { *((raw + 16) as *const i64) };
                for i in 0..len {
                    if i > 0 { out.push_str(", "); }
                    let elem = unsafe { *((data_ptr + i * 8) as *const i64) };
                    let _ = write!(out, "{}", elem);
                }
            }
            out.push(']');
        }
        _ => { let _ = write!(out, "{}", raw); }
    }
}

extern "C" fn host_map_new() -> i64 {
    let m = Box::new(ManagedMap {
        rc: 1,
        val_kind: 0,
        key_print_kind: PK_OTHER,
        val_print_kind: PK_OTHER,
        inner: std::collections::HashMap::new(),
        str_key_origs: std::collections::HashMap::new(),
    });
    Box::into_raw(m) as i64
}

extern "C" fn host_map_set_print_kinds(map: i64, key_kind: i64, val_kind: i64) {
    if map == 0 {
        return;
    }
    let m = unsafe { &mut *(map as *mut ManagedMap) };
    m.key_print_kind = key_kind;
    m.val_print_kind = val_kind;
}

extern "C" fn host_print_map(map_ptr: i64) {
    let mut out = String::new();
    if map_ptr == 0 {
        out.push_str("{}");
        print!("{out}");
        return;
    }
    let m = unsafe { &*(map_ptr as *const ManagedMap) };
    let mut entries: Vec<(i64, i64)> = m
        .inner
        .iter()
        .map(|(k, &v)| (map_key_to_raw(k), v))
        .collect();
    let kk = m.key_print_kind;
    let vk = m.val_print_kind;
    entries.sort_by(|a, b| {
        let mut sa = String::new();
        let mut sb = String::new();
        format_kind_id(&mut sa, kk, a.0);
        format_kind_id(&mut sb, kk, b.0);
        sa.cmp(&sb)
    });
    out.push('{');
    for (i, (k, v)) in entries.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        format_kind_id(&mut out, kk, *k);
        out.push_str(": ");
        format_kind_id(&mut out, vk, *v);
    }
    out.push('}');
    print!("{out}");
}

extern "C" fn host_map_get(map: i64, key: i64) -> i64 {
    if map == 0 {
        return 0;
    }
    let m = unsafe { &*(map as *const ManagedMap) };
    let mk = raw_to_map_key(key, m.key_print_kind);
    *m.inner.get(&mk).unwrap_or(&0)
}

extern "C" fn host_map_get_optional(map: i64, key: i64) -> i64 {
    if map == 0 {
        return 0;
    }
    let m = unsafe { &*(map as *const ManagedMap) };
    let mk = raw_to_map_key(key, m.key_print_kind);
    match m.inner.get(&mk) {
        Some(&v) => {
            let cell = host_mir_alloc(24) as *mut i64;
            unsafe {
                *cell = v;
                *cell.add(1) = 1;
                *cell.add(2) = m.val_kind;
            }
            cell as i64
        }
        None => 0,
    }
}

extern "C" fn host_map_set(map: i64, key: i64, value: i64) {
    if map == 0 {
        return;
    }
    let m = unsafe { &mut *(map as *mut ManagedMap) };
    let mk = raw_to_map_key(key, m.key_print_kind);
    if m.key_print_kind == PK_STR && key != 0 {
        m.str_key_origs.entry(mk.clone()).or_insert(key);
    }
    let val_kind = m.val_kind;
    let prev = m.inner.insert(mk, value);
    // Releasing the displaced value when the map's value side is
    // tagged as Object — without this, `m["k"] = v1; m["k"] = v2`
    // leaks v1 (its rc never decrements, deinit never runs, and
    // its memory stays unreachable).
    if let Some(old) = prev {
        if val_kind == 1 {
            release_object(old);
        }
    }
}

extern "C" fn host_map_set_object_value(map: i64) {
    // Marks the map's value side as Object so cascade-release on drop
    // calls deinit on each stored value.
    if map == 0 {
        return;
    }
    let m = unsafe { &mut *(map as *mut ManagedMap) };
    m.val_kind = 1;
}

extern "C" fn host_release_map(map: i64) {
    if map == 0 {
        return;
    }
    let m_mut = unsafe { &mut *(map as *mut ManagedMap) };
    if m_mut.rc <= 0 {
        return;
    }
    m_mut.rc -= 1;
    if m_mut.rc != 0 {
        return;
    }
    if m_mut.val_kind == 1 {
        let values: Vec<i64> = m_mut.inner.values().copied().collect();
        for v in values {
            release_object(v);
        }
    }
    // Reclaim the box so the HashMap drops its allocation.
    unsafe {
        let _ = Box::from_raw(map as *mut ManagedMap);
    }
}

extern "C" fn host_retain_map(map: i64) {
    if map == 0 {
        return;
    }
    let m = unsafe { &mut *(map as *mut ManagedMap) };
    if m.rc <= 0 {
        return;
    }
    m.rc += 1;
}

// String runtime helpers. Each ilang string lives on the heap as
//   [ i64 length ][ UTF-8 bytes ][ \0 ]
// and the user-visible pointer points at the first UTF-8 byte. The
// length prefix lets reads survive embedded NULs (e.g. `"a\0b"` has
// length 3); the trailing NUL keeps cstr-style C interop working
// (snprintf etc. read up to the first NUL, which is a documented
// truncation if the user puts NULs inside the string).
unsafe fn cstr_bytes<'a>(p: i64) -> &'a [u8] { unsafe {
    if p == 0 {
        return &[];
    }
    let len = *((p - 8) as *const i64);
    if len <= 0 {
        return &[];
    }
    std::slice::from_raw_parts(p as *const u8, len as usize)
}}

/// Raw C-string scanner — for pointers crossing the FFI boundary
/// from C land (e.g. `getenv()`, char** array elements). These have
/// no length prefix; we walk to the first NUL.
unsafe fn raw_cstr_bytes<'a>(p: i64) -> &'a [u8] { unsafe {
    if p == 0 {
        return &[];
    }
    let mut len = 0;
    let q = p as *const u8;
    while *q.add(len) != 0 {
        len += 1;
    }
    std::slice::from_raw_parts(q, len)
}}

extern "C" fn host_str_length(p: i64) -> i64 {
    let bytes = unsafe { cstr_bytes(p) };
    // Unicode code-point count to match `String.length` semantics.
    std::str::from_utf8(bytes)
        .map(|s| s.chars().count() as i64)
        .unwrap_or(bytes.len() as i64)
}

/// Refcount-managed heap-string registry. Strings produced by
/// `leak_cstring` (and therefore by `host_int_to_string`,
/// `host_str_concat`, `host_string_from_cstr`, …) are tracked here
/// so `host_release_string` can drop the underlying buffer once rc
/// reaches 0. Pointers we don't own (`string_data` literal symbols
/// baked into the JIT module, FFI-returned `*const char` that the
/// program treats as `string`, etc.) simply miss the registry and
/// the release becomes a no-op — the same conservative direction
/// every other heap-typed Release takes.
struct StringEntry {
    /// Owns the underlying buffer; dropped (and freed) when the entry
    /// is removed from the registry. Never read directly — the JIT
    /// reaches the bytes via the `body_ptr` map key.
    #[allow(dead_code)]
    backing: Box<[u8]>,
    rc: i64,
}
static STRING_REGISTRY: OnceLock<Mutex<HashMap<i64, StringEntry>>> = OnceLock::new();

fn string_registry_lock() -> &'static Mutex<HashMap<i64, StringEntry>> {
    STRING_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Build a heap string with the documented `[ i64 len | bytes | \0 ]`
/// layout. The returned i64 is the user-visible pointer (points at
/// the first UTF-8 byte). The backing buffer is registered with
/// `STRING_REGISTRY` at rc=1; a matching `__release_string` call
/// drops it.
fn leak_cstring(s: String) -> i64 {
    let body = s.into_bytes();
    let len = body.len() as i64;
    let mut bytes: Vec<u8> = Vec::with_capacity(8 + body.len() + 1);
    bytes.extend_from_slice(&len.to_le_bytes());
    bytes.extend_from_slice(&body);
    bytes.push(0);
    let boxed = bytes.into_boxed_slice();
    let body_ptr = unsafe { boxed.as_ptr().add(8) } as i64;
    {
        let mut reg = string_registry_lock()
            .lock()
            .expect("string registry poisoned");
        reg.insert(body_ptr, StringEntry { backing: boxed, rc: 1 });
    }
    body_ptr
}

extern "C" fn host_retain_string(p: i64) {
    if p == 0 {
        return;
    }
    let mut reg = string_registry_lock()
        .lock()
        .expect("string registry poisoned");
    if let Some(e) = reg.get_mut(&p) {
        e.rc += 1;
    }
}

extern "C" fn host_release_string(p: i64) {
    if p == 0 {
        return;
    }
    let mut reg = string_registry_lock()
        .lock()
        .expect("string registry poisoned");
    let drop_it = if let Some(e) = reg.get_mut(&p) {
        e.rc -= 1;
        e.rc <= 0
    } else {
        false
    };
    if drop_it {
        reg.remove(&p);
    }
}

extern "C" fn host_str_concat(a: i64, b: i64) -> i64 {
    let sa = unsafe { cstr_bytes(a) };
    let sb = unsafe { cstr_bytes(b) };
    let mut out = Vec::with_capacity(sa.len() + sb.len());
    out.extend_from_slice(sa);
    out.extend_from_slice(sb);
    leak_cstring(String::from_utf8_lossy(&out).into_owned())
}

extern "C" fn host_str_eq(a: i64, b: i64) -> i64 {
    if a == b {
        return 1;
    }
    let sa = unsafe { cstr_bytes(a) };
    let sb = unsafe { cstr_bytes(b) };
    if sa == sb { 1 } else { 0 }
}

extern "C" fn host_int_to_string(n: i64) -> i64 {
    leak_cstring(n.to_string())
}

extern "C" fn host_bool_to_string(b: i64) -> i64 {
    leak_cstring(if b != 0 { "true".to_string() } else { "false".to_string() })
}

fn cstr_to_str<'a>(p: i64) -> &'a str {
    let bytes = unsafe { cstr_bytes(p) };
    std::str::from_utf8(bytes).unwrap_or("")
}

extern "C" fn host_str_to_upper(p: i64) -> i64 {
    leak_cstring(cstr_to_str(p).to_uppercase())
}
extern "C" fn host_str_to_lower(p: i64) -> i64 {
    leak_cstring(cstr_to_str(p).to_lowercase())
}
extern "C" fn host_str_trim(p: i64) -> i64 {
    leak_cstring(cstr_to_str(p).trim().to_string())
}
extern "C" fn host_str_includes(p: i64, q: i64) -> i64 {
    if cstr_to_str(p).contains(cstr_to_str(q)) { 1 } else { 0 }
}
extern "C" fn host_str_starts_with(p: i64, q: i64) -> i64 {
    if cstr_to_str(p).starts_with(cstr_to_str(q)) { 1 } else { 0 }
}
extern "C" fn host_str_ends_with(p: i64, q: i64) -> i64 {
    if cstr_to_str(p).ends_with(cstr_to_str(q)) { 1 } else { 0 }
}
extern "C" fn host_str_char_at(p: i64, idx: i64) -> i64 {
    let s = cstr_to_str(p);
    let c = s.chars().nth(idx as usize);
    leak_cstring(c.map(|c| c.to_string()).unwrap_or_default())
}
extern "C" fn host_str_slice(p: i64, start: i64, end: i64) -> i64 {
    let s = cstr_to_str(p);
    let chars: Vec<char> = s.chars().collect();
    let lo = (start.max(0) as usize).min(chars.len());
    let hi = (end.max(0) as usize).min(chars.len());
    let lo = lo.min(hi);
    leak_cstring(chars[lo..hi].iter().collect::<String>())
}
extern "C" fn host_str_replace(p: i64, from: i64, to: i64) -> i64 {
    let s = cstr_to_str(p);
    let f = cstr_to_str(from);
    let t = cstr_to_str(to);
    leak_cstring(s.replace(f, t))
}

// Array layout: 3-i64 header [length | capacity | data_ptr] where
// data_ptr → i64×capacity. The host helpers below treat each data
// cell as an opaque i64.
unsafe fn array_header(arr: i64) -> (i64, i64, i64) { unsafe {
    let p = arr as *const i64;
    (*p, *p.add(1), *p.add(2))
}}

extern "C" fn host_array_index_of(arr: i64, value: i64) -> i64 {
    if arr == 0 {
        return -1;
    }
    let (len, _cap, data) = unsafe { array_header(arr) };
    for i in 0..len {
        let cell = unsafe { *((data + i * 8) as *const i64) };
        if cell == value {
            return i;
        }
    }
    -1
}

extern "C" fn host_array_includes(arr: i64, value: i64) -> i64 {
    if host_array_index_of(arr, value) >= 0 { 1 } else { 0 }
}

/// Write a value into a packed array slot at `data + idx*stride`,
/// truncating the i64 source down to the stride width.
unsafe fn store_packed(data: i64, idx: i64, stride: i64, value: i64) {
    unsafe {
        let addr = (data + idx * stride) as *mut u8;
        match stride {
            1 => *(addr as *mut u8) = value as u8,
            2 => *(addr as *mut u16) = value as u16,
            4 => *(addr as *mut u32) = value as u32,
            _ => *(addr as *mut i64) = value,
        }
    }
}

extern "C" fn host_array_push(arr: i64, value: i64) {
    if arr == 0 {
        return;
    }
    unsafe {
        let h = arr as *mut i64;
        let len = *h;
        let cap = *h.add(1);
        let data = *h.add(2);
        let stride = *h.add(5);
        if len < cap {
            store_packed(data, len, stride, value);
            *h = len + 1;
        } else {
            let new_cap = (cap * 2).max(4);
            let new_data = host_mir_alloc(new_cap * stride);
            std::ptr::copy_nonoverlapping(
                data as *const u8,
                new_data as *mut u8,
                (len * stride) as usize,
            );
            store_packed(new_data, len, stride, value);
            // Free the old data buffer — without this, every grow
            // leaks the previous backing store. log2(N) grows for
            // N pushes, so a long loop accumulates ~2*N*stride
            // unreachable bytes.
            if data != 0 && cap > 0 {
                host_mir_free(data, cap * stride);
            }
            *h = len + 1;
            *h.add(1) = new_cap;
            *h.add(2) = new_data;
        }
    }
}

/// Construct a new i64-cell array (48-byte header, stride 8) from an
/// i64 slice. Used by helpers that produce string[] / i64[] results.
/// `elem_kind` should be the KIND_* tag for the element type so
/// `host_release_array` can cascade-release the contents on drop
/// (e.g. KIND_STR for split() results, KIND_OBJECT for
/// Map<_, ClassT>.values()).
fn build_array(items: &[i64], elem_kind: i64) -> i64 {
    let cap = items.len().max(4);
    let header = host_mir_alloc(48);
    let data = host_mir_alloc((cap * 8) as i64);
    unsafe {
        let h = header as *mut i64;
        *h = items.len() as i64;
        *h.add(1) = cap as i64;
        *h.add(2) = data;
        *h.add(3) = 1; // rc
        *h.add(4) = elem_kind;
        *h.add(5) = 8; // stride
        for (i, v) in items.iter().enumerate() {
            *((data + (i as i64) * 8) as *mut i64) = *v;
        }
    }
    header
}

/// Invoke a closure (`[fn_ptr | captures...]` block pointer) with one
/// arg and the trailing env pointer. The fn signature follows the
/// unified ABI: `extern "C" fn(arg, env_ptr) -> i64`.
unsafe fn call_closure_1(closure: i64, arg: i64) -> i64 { unsafe {
    let fn_ptr = *(closure as *const i64);
    let f: extern "C" fn(i64, i64) -> i64 = std::mem::transmute(fn_ptr);
    f(arg, closure)
}}

/// `arrayFromCArray<T>(src, n, stride, kind_tag)` — copy `n × stride`
/// bytes from a C-side array into a fresh ilang dyn-array `T[]`.
/// The lower side picks `stride` from T's MirTy so the host doesn't
/// need to know T.
extern "C" fn host_c_array_to_array(src: i64, n: i64, stride: i64, kind_tag: i64) -> i64 {
    let n_safe = if n < 0 { 0 } else { n };
    let bytes = n_safe * stride;
    let header = host_mir_alloc(48);
    let data = host_mir_alloc(bytes.max(stride));
    unsafe {
        if bytes > 0 && src != 0 {
            std::ptr::copy_nonoverlapping(src as *const u8, data as *mut u8, bytes as usize);
        }
        let h = header as *mut i64;
        *h = n_safe;
        *h.add(1) = n_safe;
        *h.add(2) = data;
        *h.add(3) = 1;
        *h.add(4) = kind_tag;
        *h.add(5) = stride;
    }
    header
}

// `readT(p: *const void, offset: i64): T` family — primitive load
// at `p + offset` (offset in bytes). Used by FFI bindings to peek
// fields off opaque C structs whose layout the language doesn't
// know. Bytes-narrower-than-i64 values zero-extend (unsigned) or
// sign-extend (signed) on return so callers see the right value
// after the i64 boxing the cross-FFI call performs.
extern "C" fn host_read_i8(p: i64, off: i64) -> i64 {
    unsafe { *((p + off) as *const i8) as i64 }
}
extern "C" fn host_read_i16(p: i64, off: i64) -> i64 {
    unsafe { (*((p + off) as *const i16)) as i64 }
}
extern "C" fn host_read_i32(p: i64, off: i64) -> i64 {
    unsafe { (*((p + off) as *const i32)) as i64 }
}
extern "C" fn host_read_i64(p: i64, off: i64) -> i64 {
    unsafe { *((p + off) as *const i64) }
}
extern "C" fn host_read_u8(p: i64, off: i64) -> i64 {
    unsafe { (*((p + off) as *const u8)) as i64 }
}
extern "C" fn host_read_u16(p: i64, off: i64) -> i64 {
    unsafe { (*((p + off) as *const u16)) as i64 }
}
extern "C" fn host_read_u32(p: i64, off: i64) -> i64 {
    unsafe { (*((p + off) as *const u32)) as i64 }
}
extern "C" fn host_read_u64(p: i64, off: i64) -> i64 {
    unsafe { *((p + off) as *const u64) as i64 }
}
extern "C" fn host_read_f32(p: i64, off: i64) -> f32 {
    unsafe { *((p + off) as *const f32) }
}
extern "C" fn host_read_f64(p: i64, off: i64) -> f64 {
    unsafe { *((p + off) as *const f64) }
}

// `writeT(p: *void, offset: i64, value: T)` family — paired
// primitive store at `p + offset`. The value comes in as the wider
// host-ABI type; the host helper truncates as needed.
extern "C" fn host_write_i8(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut i8) = v as i8; }
}
extern "C" fn host_write_i16(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut i16) = v as i16; }
}
extern "C" fn host_write_i32(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut i32) = v as i32; }
}
extern "C" fn host_write_i64(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut i64) = v; }
}
extern "C" fn host_write_u8(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut u8) = v as u8; }
}
extern "C" fn host_write_u16(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut u16) = v as u16; }
}
extern "C" fn host_write_u32(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut u32) = v as u32; }
}
extern "C" fn host_write_u64(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut u64) = v as u64; }
}
extern "C" fn host_write_f32(p: i64, off: i64, v: f32) {
    unsafe { *((p + off) as *mut f32) = v; }
}
extern "C" fn host_write_f64(p: i64, off: i64, v: f64) {
    unsafe { *((p + off) as *mut f64) = v; }
}

/// Box a raw discriminant value into a unit-variant enum heap cell.
/// Layout matches `Inst::NewEnum` for unit variants: 8 B containing
/// the tag at offset 0, no payload. Used by the integer→enum
/// coerce path.
/// REPL host-slot storage for cross-chunk top-level `let` values.
/// The REPL session assigns each top-level binding a stable slot
/// index; the JIT-compiled chunk reads / writes through these
/// helpers so values survive across module rebuilds. Lives outside
/// any JITModule so freeing & recompiling per chunk is safe.
fn repl_slots() -> &'static std::sync::Mutex<Vec<i64>> {
    use std::sync::OnceLock;
    static SLOTS: OnceLock<std::sync::Mutex<Vec<i64>>> = OnceLock::new();
    SLOTS.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}

extern "C" fn host_repl_load_slot(idx: i64) -> i64 {
    let g = repl_slots().lock().expect("repl slots poisoned");
    g.get(idx as usize).copied().unwrap_or(0)
}

extern "C" fn host_repl_store_slot(idx: i64, value: i64) {
    let mut g = repl_slots().lock().expect("repl slots poisoned");
    let need = (idx as usize) + 1;
    if g.len() < need {
        g.resize(need, 0);
    }
    g[idx as usize] = value;
}

/// Public reset hook so REPL sessions starting fresh don't carry
/// over slots from a previous in-process run (mostly useful for
/// tests).
pub fn reset_repl_slots() {
    let mut g = repl_slots().lock().expect("repl slots poisoned");
    g.clear();
}

extern "C" fn host_enum_box(disc: i64) -> i64 {
    let p = host_mir_alloc(8);
    unsafe { *(p as *mut i64) = disc; }
    p
}

/// rc-tracked registry for enum payload-variant cells. Unit-variant
/// cells go through `__enum_unit_get` instead (interned) and are
/// NOT tracked here. Cells in this registry are freed when their
/// rc reaches 0 — the cascade-on-cell-drop for any heap payload
/// content is not yet implemented (conservative: cell is freed,
/// heap payload leaks until match-time extraction retain + cell
/// release cascade are wired together).
struct EnumEntry {
    rc: i64,
    total_bytes: i64,
    /// Global enum id — used by `host_release_enum`'s cascade
    /// path to look up the variant's payload kinds via
    /// `enum_info_lock` and recursively release each heap field.
    global_eid: u32,
}
static ENUM_REGISTRY: OnceLock<Mutex<HashMap<i64, EnumEntry>>> = OnceLock::new();

fn enum_registry_lock() -> &'static Mutex<HashMap<i64, EnumEntry>> {
    ENUM_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// `__enum_alloc(global_eid, n_payload, disc)` — allocate an enum
/// payload-variant cell, register with rc=1, write the discriminant
/// at offset 0. Returns the ptr the lower side then writes payload
/// fields into (offsets 8 + i*8). NewEnum codegen invokes this
/// instead of the bare alloc + tag-write for `n_payload > 0`.
extern "C" fn host_enum_alloc(global_eid: i64, n_payload: i64, disc: i64) -> i64 {
    let total = (1 + n_payload) * 8;
    let ptr = host_mir_alloc(total);
    unsafe { *(ptr as *mut i64) = disc; }
    let mut reg = enum_registry_lock()
        .lock()
        .expect("enum registry poisoned");
    reg.insert(ptr, EnumEntry { rc: 1, total_bytes: total, global_eid: global_eid as u32 });
    ptr
}

extern "C" fn host_retain_enum(p: i64) {
    if p == 0 {
        return;
    }
    let mut reg = enum_registry_lock()
        .lock()
        .expect("enum registry poisoned");
    if let Some(e) = reg.get_mut(&p) {
        e.rc += 1;
    }
    // Unit-variant cells (interned via __enum_unit_get) miss the
    // registry — the retain becomes a no-op, which is what we want.
}

extern "C" fn host_release_enum(p: i64) {
    if p == 0 {
        return;
    }
    let mut reg = enum_registry_lock()
        .lock()
        .expect("enum registry poisoned");
    let to_free = if let Some(e) = reg.get_mut(&p) {
        e.rc -= 1;
        if e.rc <= 0 {
            Some((e.total_bytes, e.global_eid))
        } else {
            None
        }
    } else {
        None
    };
    if let Some((total, global_eid)) = to_free {
        reg.remove(&p);
        drop(reg);
        // Cascade-release any heap-typed payload before freeing the
        // cell. Look up the variant's payload kinds via the global
        // enum-info table, walk each payload slot, dispatch
        // release_value_by_kind. Pairs with the EnumPayload
        // codegen's extraction-side retain so the rc bookkeeping
        // stays balanced when the user pattern-matched the variant.
        let tag = unsafe { *(p as *const i64) };
        let kinds = {
            let info = enum_info_lock()
                .lock()
                .expect("enum info poisoned");
            info.get(&global_eid)
                .and_then(|ei| ei.variants.get(&tag))
                .map(|(_name, k)| k.clone())
        };
        if let Some(kinds) = kinds {
            for (i, kind) in kinds.iter().enumerate() {
                let raw = unsafe { *((p + 8 + (i as i64) * 8) as *const i64) };
                release_print_kind(raw, kind);
            }
        }
        host_mir_free(p, total);
    }
}

/// Cached unit-variant enum cell. Same `(global_enum_id, disc)`
/// always returns the same pointer, so per-frame `sdl.Axis.leftX`
/// style enum-ctors don't keep allocating fresh 8-byte cells. The
/// cells are leaked on purpose — they're program-lifetime constants.
static ENUM_UNIT_CACHE: OnceLock<Mutex<HashMap<(u32, i64), i64>>> = OnceLock::new();

fn enum_unit_cache_lock() -> &'static Mutex<HashMap<(u32, i64), i64>> {
    ENUM_UNIT_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

extern "C" fn host_enum_unit_get(global_eid: i64, disc: i64) -> i64 {
    let key = (global_eid as u32, disc);
    {
        let m = enum_unit_cache_lock().lock().expect("enum unit cache poisoned");
        if let Some(&p) = m.get(&key) {
            return p;
        }
    }
    // Race-permissive: if two threads insert concurrently the second
    // overwrites with a fresh leaked cell of the same value — harmless
    // since the cells are immutable singletons and both leak.
    let p = host_mir_alloc(8);
    unsafe { *(p as *mut i64) = disc; }
    let mut m = enum_unit_cache_lock().lock().expect("enum unit cache poisoned");
    *m.entry(key).or_insert(p)
}

/// Wrap an inline fixed-length array (a bare `ptr` to `len` elements
/// of `stride` bytes each) into a dynamic-array header that the
/// host_array_* helpers expect. Used at builtin call sites where the
/// MIR arg type is `Array { len: Some(n), .. }` — those have no
/// header, just the raw element block. The wrapper is freshly heap-
/// allocated and considered owned by the caller (release rules apply
/// as for any other freshly-built array).
extern "C" fn host_fixed_to_dyn(ptr: i64, len: i64, stride: i64, kind_tag: i64) -> i64 {
    let header = host_mir_alloc(48);
    unsafe {
        let h = header as *mut i64;
        *h = len;            // len
        *h.add(1) = len;     // cap
        *h.add(2) = ptr;     // data_ptr (alias — no copy)
        *h.add(3) = 1;       // rc
        *h.add(4) = kind_tag;
        *h.add(5) = stride;
    }
    header
}

extern "C" fn host_array_map(arr: i64, closure: i64, result_kind: i64) -> i64 {
    if arr == 0 || closure == 0 {
        return build_array(&[], result_kind);
    }
    let (len, _cap, data) = unsafe { array_header(arr) };
    let mut out = Vec::with_capacity(len as usize);
    for i in 0..len {
        let cell = unsafe { *((data + i * 8) as *const i64) };
        let v = unsafe { call_closure_1(closure, cell) };
        out.push(v);
    }
    // result_kind is the closure's return MirTy's KIND_* tag,
    // threaded in by the lower side. Lets the result array's
    // drop cascade-release each closure-produced value.
    build_array(&out, result_kind)
}

extern "C" fn host_array_filter(arr: i64, closure: i64) -> i64 {
    if arr == 0 || closure == 0 {
        return build_array(&[], KIND_NONE);
    }
    let (len, _cap, data) = unsafe { array_header(arr) };
    let elem_kind = unsafe { *((arr + 32) as *const i64) };
    let mut out = Vec::new();
    for i in 0..len {
        let cell = unsafe { *((data + i * 8) as *const i64) };
        let keep = unsafe { call_closure_1(closure, cell) };
        if keep != 0 {
            // Filter passes through source elements unchanged —
            // share their +1 by retaining the kept ones so both
            // the source array (when it drops) and the result
            // array (when it drops) account for the reference.
            if elem_kind != KIND_NONE {
                retain_by_kind(cell, elem_kind);
            }
            out.push(cell);
        }
    }
    build_array(&out, elem_kind)
}

extern "C" fn host_array_slice(arr: i64, start: i64, end: i64) -> i64 {
    if arr == 0 {
        return build_array(&[], KIND_NONE);
    }
    let (len, _cap, data) = unsafe { array_header(arr) };
    let elem_kind = unsafe { *((arr + 32) as *const i64) };
    let lo = start.max(0).min(len) as usize;
    let hi = end.max(0).min(len) as usize;
    let lo = lo.min(hi);
    let mut out: Vec<i64> = Vec::with_capacity(hi - lo);
    for i in lo..hi {
        let cell = unsafe { *((data + (i as i64) * 8) as *const i64) };
        // Slice copies element references — retain so both arrays
        // own the reference (mirrors filter).
        if elem_kind != KIND_NONE {
            retain_by_kind(cell, elem_kind);
        }
        out.push(cell);
    }
    build_array(&out, elem_kind)
}

extern "C" fn host_array_for_each(arr: i64, closure: i64) {
    if arr == 0 || closure == 0 {
        return;
    }
    let (len, _cap, data) = unsafe { array_header(arr) };
    for i in 0..len {
        let cell = unsafe { *((data + i * 8) as *const i64) };
        unsafe { call_closure_1(closure, cell) };
    }
}

extern "C" fn host_str_split(p: i64, sep: i64) -> i64 {
    let s = cstr_to_str(p);
    let sp = cstr_to_str(sep);
    let parts: Vec<i64> = if sp.is_empty() {
        // Empty separator → split per character (matching syntax.md).
        s.chars().map(|c| leak_cstring(c.to_string())).collect()
    } else {
        s.split(sp).map(|t| leak_cstring(t.to_string())).collect()
    };
    // Each part is a fresh leak_cstring entry — tag the array as
    // KIND_STR so dropping it cascades release_string and reclaims
    // every part.
    build_array(&parts, KIND_STR)
}

extern "C" fn host_array_pop(arr: i64) -> i64 {
    // Returns the popped value as Optional<T>: a 3-cell heap
    // [value | rc | kind_tag], or 0 (none). Inherits the array's
    // elem kind tag so cascade deinit works on Optional drop.
    if arr == 0 {
        return 0;
    }
    unsafe {
        let h = arr as *mut i64;
        let len = *h;
        if len == 0 {
            return 0;
        }
        let data = *h.add(2);
        let stride = *h.add(5);
        let addr = (data + (len - 1) * stride) as *const u8;
        let v: i64 = match stride {
            1 => *(addr as *const u8) as i64,
            2 => *(addr as *const u16) as i64,
            4 => *(addr as *const u32) as i64,
            _ => *(addr as *const i64),
        };
        *h = len - 1;
        let elem_tag = *h.add(4);
        let cell = host_mir_alloc(24) as *mut i64;
        *cell = v;
        *cell.add(1) = 1;
        *cell.add(2) = elem_tag;
        cell as i64
    }
}

/// `__array_data_ptr(arr)` — return the i64 byte address of the
/// array's data buffer (header offset 16 holds it).
extern "C" fn host_array_data_ptr(arr: i64) -> i64 {
    if arr == 0 {
        return 0;
    }
    unsafe { *((arr + 16) as *const i64) }
}

/// Byte stride of an array element type. 1 / 2 / 4 / 8 — anything
/// not one of the small numeric types lands on the i64 cell.
fn elem_byte_stride(t: &MirTy) -> i64 {
    match t {
        MirTy::I8 | MirTy::U8 | MirTy::CChar | MirTy::Bool => 1,
        MirTy::I16 | MirTy::U16 => 2,
        MirTy::I32 | MirTy::U32 | MirTy::F32 => 4,
        _ => 8,
    }
}

/// Cranelift type to use for a packed array load/store of `t`. Only
/// the small numeric types get tight packing; everything else uses
/// the i64 cell path (returns `None`).
fn elem_clif_type(t: &MirTy) -> Option<cranelift::prelude::Type> {
    use cranelift::prelude::types as ct;
    match t {
        MirTy::I8 | MirTy::U8 | MirTy::CChar | MirTy::Bool => Some(ct::I8),
        MirTy::I16 | MirTy::U16 => Some(ct::I16),
        MirTy::I32 | MirTy::U32 => Some(ct::I32),
        MirTy::F32 => Some(ct::F32),
        MirTy::F64 => Some(ct::F64),
        _ => None,
    }
}

/// Truncate a Cranelift value to fit the target type if it is wider;
/// otherwise pass through (assumes the source already matches).
fn ireduce_or_pass(
    fb: &mut ClifFnBuilder,
    v: cranelift::prelude::Value,
    target: cranelift::prelude::Type,
) -> cranelift::prelude::Value {
    let cur = fb.func.dfg.value_type(v);
    if cur == target {
        return v;
    }
    if target.is_int() && cur.is_int() {
        if cur.bits() > target.bits() {
            return fb.ins().ireduce(target, v);
        }
        if cur.bits() < target.bits() {
            return fb.ins().uextend(target, v);
        }
    }
    v
}

extern "C" fn host_identity(p: i64) -> i64 { p }

/// Stub for `@extern(C) @optional` fns whose lib / symbol couldn't
/// be resolved. Aborts if called; user code is expected to gate
/// via `os.libLoaded(...)`.
extern "C" fn host_optional_missing_stub() -> ! {
    eprintln!(
        "panic: invoked an `@extern(C) @optional` fn whose library was not loaded"
    );
    std::process::exit(1);
}

unsafe extern "C" {
    fn dlsym(handle: *mut u8, name: *const u8) -> *mut u8;
}

// `RTLD_DEFAULT` differs by platform: macOS uses (-2 as *mut u8),
// Linux uses NULL. Use a const fn so each target picks the right
// sentinel.
#[cfg(target_os = "macos")]
const RTLD_DEFAULT: *mut u8 = -2isize as *mut u8;
#[cfg(not(target_os = "macos"))]
const RTLD_DEFAULT: *mut u8 = std::ptr::null_mut();

fn process_symbol_exists(name: &str) -> bool {
    let mut nul = name.as_bytes().to_vec();
    nul.push(0);
    let p = unsafe { dlsym(RTLD_DEFAULT, nul.as_ptr()) };
    !p.is_null()
}

/// `stringFromCstr(p)` — copy the bytes pointed to by `p` into a
/// fresh leaked NUL-terminated buffer so `free(p)` afterwards
/// doesn't invalidate the caller's string view.
extern "C" fn host_string_from_cstr(p: i64) -> i64 {
    if p == 0 {
        return leak_cstring(String::new());
    }
    let bytes = unsafe { raw_cstr_bytes(p) };
    leak_cstring(String::from_utf8_lossy(bytes).into_owned())
}
extern "C" fn host_noop(_: i64) {}

/// `cstrArrayToStrings(p: *const *const char): string[]` — walk a
/// NULL-terminated `char**` and copy each `char*` into a fresh
/// NUL-terminated buffer, packed into a 40-byte-header ilang array.
extern "C" fn host_cstr_array_to_strings(ptrs: i64) -> i64 {
    let mut elems: Vec<i64> = Vec::new();
    if ptrs != 0 {
        unsafe {
            let mut p = ptrs as *const *const u8;
            while !(*p).is_null() {
                let raw = (*p) as i64;
                let bytes = raw_cstr_bytes(raw);
                let s = String::from_utf8_lossy(bytes).into_owned();
                elems.push(leak_cstring(s));
                p = p.add(1);
            }
        }
    }
    let n = elems.len() as i64;
    let header = host_mir_alloc(48);
    let data = host_mir_alloc(n.max(1) * 8);
    unsafe {
        let h = header as *mut i64;
        *h = n;
        *h.add(1) = n;
        *h.add(2) = data;
        *h.add(3) = 1;
        *h.add(4) = 0;
        *h.add(5) = 8; // stride
        let d = data as *mut i64;
        for (i, s) in elems.iter().enumerate() {
            *d.add(i) = *s;
        }
    }
    header
}

extern "C" fn host_errno_check_i32(rc: i32) -> i64 {
    // Returns Optional<i32> as a heap cell: 0 = none, ptr = some(rc).
    if rc < 0 {
        return 0;
    }
    let cell = host_mir_alloc(8) as *mut i32;
    unsafe {
        *cell = rc;
    }
    cell as i64
}

extern "C" fn host_errno_check_i64(rc: i64) -> i64 {
    if rc < 0 {
        return 0;
    }
    let cell = host_mir_alloc(8) as *mut i64;
    unsafe {
        *cell = rc;
    }
    cell as i64
}

/// `os.libLoaded(name)` — true if the JIT could resolve symbols
/// from `name` (or any fallback in its `@lib(...)` group). The
/// mir-codegen pipeline relies on Cranelift JIT's process-wide
/// symbol search, which always succeeds for libc-provided names,
/// so the contract reduces to: returning true matches what the
/// fallback paths do.
/// `os.libLoaded(name)` — try to dlopen the library on demand and
/// remember whether it succeeded. The mir-codegen pipeline relies on
/// Cranelift JIT's process-wide symbol search, which always succeeds
/// for libc-provided names; for fallback libs declared via
/// `@lib("primary", "fallback")` we attempt each in turn so the
/// `os.libLoaded` query reflects reality.
extern "C" fn host_os_lib_loaded(name: i64) -> i64 {
    let n = if name == 0 {
        return 0;
    } else {
        let bytes = unsafe { cstr_bytes(name) };
        String::from_utf8_lossy(bytes).into_owned()
    };
    // First, check whether `name` itself opens.
    if try_open_lib(&n).is_some() {
        return 1;
    }
    // Otherwise, check fallback groups: any `@lib(a, b, c)` group
    // containing `n` whose other entry opens counts as loaded.
    let registry = lib_groups_lock().lock().expect("lib groups poisoned");
    for group in registry.iter() {
        if !group.iter().any(|s| s.as_str() == n) {
            continue;
        }
        for alt in group {
            let s = alt.as_str();
            if s == n {
                continue;
            }
            if try_open_lib(s).is_some() {
                return 1;
            }
        }
    }
    0
}

extern "C" fn host_os_lib_load_error(name: i64) -> i64 {
    let n = if name == 0 {
        return leak_cstring(String::new());
    } else {
        let bytes = unsafe { cstr_bytes(name) };
        String::from_utf8_lossy(bytes).into_owned()
    };
    match try_open_lib_err(&n) {
        Some(e) => leak_cstring(e),
        None => leak_cstring(String::new()),
    }
}

unsafe extern "C" {
    fn dlopen(path: *const u8, flags: i32) -> *mut u8;
    fn dlerror() -> *const u8;
}

static LIB_GROUPS: OnceLock<Mutex<Vec<Vec<ilang_ast::Symbol>>>> = OnceLock::new();

fn lib_groups_lock() -> &'static Mutex<Vec<Vec<ilang_ast::Symbol>>> {
    LIB_GROUPS.get_or_init(|| Mutex::new(Vec::new()))
}

const RTLD_LAZY: i32 = 1;

fn try_open_lib(name: &str) -> Option<*mut u8> {
    let try_one = |n: &str| -> Option<*mut u8> {
        let mut nul = n.as_bytes().to_vec();
        nul.push(0);
        let h = unsafe { dlopen(nul.as_ptr(), RTLD_LAZY) };
        if h.is_null() { None } else { Some(h) }
    };
    if let Some(h) = try_one(name) {
        return Some(h);
    }
    // Bare name like "c" / "SDL2" — try OS-specific candidate
    // filenames and Homebrew install dirs (Apple Silicon
    // `/opt/homebrew`, Intel `/usr/local`) so user-installed libs
    // resolve out of the box. Mirrors the candidates the legacy
    // `crates/ilang-codegen/src/native_extern.rs` walks.
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
            if let Some(h) = try_one(&cand) {
                return Some(h);
            }
        }
    }
    None
}

fn try_open_lib_err(name: &str) -> Option<String> {
    let mut nul = name.as_bytes().to_vec();
    nul.push(0);
    let h = unsafe { dlopen(nul.as_ptr(), RTLD_LAZY) };
    if !h.is_null() {
        return None;
    }
    unsafe {
        let p = dlerror();
        if p.is_null() {
            return Some(format!("could not load `{name}`"));
        }
        let bytes = cstr_bytes(p as i64);
        Some(String::from_utf8_lossy(bytes).into_owned())
    }
}

extern "C" fn host_os_errno() -> i32 {
    // Best-effort errno: read Rust's libc `errno`.
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

extern "C" fn host_os_set_errno(code: i32) {
    #[cfg(target_os = "macos")]
    unsafe extern "C" {
        fn __error() -> *mut i32;
    }
    #[cfg(target_os = "linux")]
    unsafe extern "C" {
        fn __errno_location() -> *mut i32;
    }
    unsafe {
        #[cfg(target_os = "macos")]
        {
            *__error() = code;
        }
        #[cfg(target_os = "linux")]
        {
            *__errno_location() = code;
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            let _ = code;
        }
    }
}

extern "C" fn host_atan2(y: f64, x: f64) -> f64 { y.atan2(x) }
extern "C" fn host_pow(x: f64, y: f64) -> f64 { x.powf(y) }
extern "C" fn host_sin(x: f64) -> f64 { x.sin() }
extern "C" fn host_cos(x: f64) -> f64 { x.cos() }
extern "C" fn host_tan(x: f64) -> f64 { x.tan() }
extern "C" fn host_asin(x: f64) -> f64 { x.asin() }
extern "C" fn host_acos(x: f64) -> f64 { x.acos() }
extern "C" fn host_atan(x: f64) -> f64 { x.atan() }
extern "C" fn host_sqrt(x: f64) -> f64 { x.sqrt() }
extern "C" fn host_exp(x: f64) -> f64 { x.exp() }
extern "C" fn host_ln(x: f64) -> f64 { x.ln() }
extern "C" fn host_log10(x: f64) -> f64 { x.log10() }
extern "C" fn host_log2(x: f64) -> f64 { x.log2() }
extern "C" fn host_floor(x: f64) -> f64 { x.floor() }
extern "C" fn host_ceil(x: f64) -> f64 { x.ceil() }
extern "C" fn host_round(x: f64) -> f64 { x.round() }
extern "C" fn host_abs(x: f64) -> f64 { x.abs() }

/// Invokes an ilang fn closure pointer as a 2-arg i32 callback.
/// The closure layout is `[fn_ptr | rc | captures...]`; we load
/// fn_ptr at offset 0 and call it with the closure as env (the
/// trailing hidden arg matches the unified ilang calling convention).
extern "C" fn host_test_apply_i32_cb(closure_ptr: i64, a: i64, b: i64) -> i32 {
    if closure_ptr == 0 {
        return 0;
    }
    let fn_addr = unsafe { *(closure_ptr as *const i64) };
    let f: extern "C" fn(i64, i64, i64) -> i32 = unsafe { std::mem::transmute(fn_addr) };
    f(a, b, closure_ptr)
}

extern "C" fn host_test_expect(actual: i64, expected: i64) {
    if actual != expected {
        eprintln!("test assertion failed: expected {expected}, got {actual}");
        std::process::exit(2);
    }
}

extern "C" fn host_test_expect_str(actual: i64, expected: i64) {
    let a = cstr_to_str(actual);
    let e = cstr_to_str(expected);
    if a != e {
        eprintln!("test assertion failed: expected {e:?}, got {a:?}");
        std::process::exit(2);
    }
}

extern "C" fn host_test_expect_bool(actual: i8, expected: i8) {
    if actual != expected {
        eprintln!(
            "test assertion failed: expected {}, got {}",
            expected != 0,
            actual != 0
        );
        std::process::exit(2);
    }
}

extern "C" fn host_test_expect_f64(actual: f64, expected: f64) {
    if (actual - expected).abs() > 1e-9 {
        eprintln!("test assertion failed: expected {expected}, got {actual}");
        std::process::exit(2);
    }
}

extern "C" fn host_test_expect_true(condition: i8) {
    if condition == 0 {
        eprintln!("test assertion failed: expected true, got false");
        std::process::exit(2);
    }
}

extern "C" fn host_test_expect_false(condition: i8) {
    if condition != 0 {
        eprintln!("test assertion failed: expected false, got true");
        std::process::exit(2);
    }
}

extern "C" fn host_test_fail(msg: i64) {
    eprintln!("test failure: {}", cstr_to_str(msg));
    std::process::exit(2);
}

extern "C" fn host_ilang_panic(msg: i64) -> ! {
    eprintln!("{}", cstr_to_str(msg));
    std::process::exit(1);
}

extern "C" fn host_print_int(n: i64) {
    print!("{}", n);
}
extern "C" fn host_print_bool(b: i64) {
    print!("{}", if b != 0 { "true" } else { "false" });
}
extern "C" fn host_print_f64(f: f64) {
    if f.fract() == 0.0 && f.is_finite() {
        print!("{:.1}", f);
    } else {
        print!("{}", f);
    }
}
extern "C" fn host_print_str(p: i64) {
    let bytes = unsafe { cstr_bytes(p) };
    let s = String::from_utf8_lossy(bytes);
    print!("{}", s);
}
extern "C" fn host_print_space() {
    print!(" ");
}
extern "C" fn host_print_newline() {
    println!();
}

extern "C" fn host_map_has(map: i64, key: i64) -> i64 {
    if map == 0 {
        return 0;
    }
    let mm = unsafe { &*(map as *const ManagedMap) };
    let mk = raw_to_map_key(key, mm.key_print_kind);
    if mm.inner.contains_key(&mk) { 1 } else { 0 }
}

extern "C" fn host_map_size(map: i64) -> i64 {
    if map == 0 {
        return 0;
    }
    let m = unsafe { &(*(map as *const ManagedMap)).inner };
    m.len() as i64
}

extern "C" fn host_map_delete(map: i64, key: i64) -> i64 {
    if map == 0 {
        return 0;
    }
    let mm = unsafe { &mut *(map as *mut ManagedMap) };
    let mk = raw_to_map_key(key, mm.key_print_kind);
    let val_kind = mm.val_kind;
    if let Some(old) = mm.inner.remove(&mk) {
        // Match the cascade-on-drop semantics — a removed key drops
        // its value just as if the whole map were released. Without
        // this, deleting a heap-valued key leaks the value.
        if val_kind == 1 {
            release_object(old);
        }
        1
    } else {
        0
    }
}

extern "C" fn host_map_keys(map: i64) -> i64 {
    if map == 0 {
        return build_array(&[], KIND_NONE);
    }
    let mm = unsafe { &*(map as *const ManagedMap) };
    // String-keyed maps return interned key pointers (str_key_origs
    // is the original `cstrFromString` user passed in for hash key
    // i.e. registry-tracked). Tag KIND_NONE for those — the keys
    // are borrowed views, not freshly allocated copies the result
    // array should free. Same for int keys (KIND_NONE).
    let v: Vec<i64> = mm
        .inner
        .keys()
        .map(|k| {
            mm.str_key_origs
                .get(k)
                .copied()
                .unwrap_or_else(|| map_key_to_raw(k))
        })
        .collect();
    build_array(&v, KIND_NONE)
}

extern "C" fn host_map_values(map: i64) -> i64 {
    if map == 0 {
        return build_array(&[], KIND_NONE);
    }
    let mm = unsafe { &*(map as *const ManagedMap) };
    let val_kind = mm.val_kind;
    let v: Vec<i64> = mm.inner.values().copied().collect();
    // Result array's kind reflects the map's value side. A
    // Map<K, ClassT>.values() should cascade release_object on
    // drop so each value's deinit fires when the result array
    // is reclaimed. Borrowed-from-map values need a retain so
    // both the source map and the result array account for the
    // reference.
    let elem_kind = if val_kind == 1 { KIND_OBJECT } else { KIND_NONE };
    if elem_kind != KIND_NONE {
        for &cell in &v {
            retain_by_kind(cell, elem_kind);
        }
    }
    build_array(&v, elem_kind)
}

fn clif_signature_for(
    module: &JITModule,
    f: &MirFunction,
    prog: &Program,
) -> Result<Signature, CompileError> {
    let mut sig = module.make_signature();
    let is_extern = matches!(f.kind, ilang_mir::FunctionKind::Extern { .. });
    // sret return: hidden first param (StructReturn), no clif return.
    let sret_size = if is_extern {
        struct_indirect(&f.ret, prog)
    } else {
        None
    };
    if sret_size.is_some() {
        sig.params.push(AbiParam::special(
            types::I64,
            cranelift_codegen::ir::ArgumentPurpose::StructReturn,
        ));
    }
    for p in f.params.iter() {
        if is_extern {
            if let Some((elem_ct, count)) = struct_hfa(&p.ty, prog) {
                for _ in 0..count {
                    sig.params.push(AbiParam::new(elem_ct));
                }
                continue;
            }
            if let Some(chunks) = struct_chunks(&p.ty, prog) {
                for _ in 0..chunks {
                    sig.params.push(AbiParam::new(types::I64));
                }
                continue;
            }
        }
        if let Some(ct) = mir_to_clif(&p.ty) {
            sig.params.push(AbiParam::new(ct));
        } else {
            return Err(CompileError::Unsupported("unit / void params"));
        }
    }
    if !is_extern {
        sig.params.push(AbiParam::new(types::I64));
    }
    if sret_size.is_some() {
        // sret: no clif-level return value; the caller's hidden
        // pointer receives the bytes.
        return Ok(sig);
    }
    if !matches!(f.ret, MirTy::Unit) {
        if is_extern {
            if let Some((elem_ct, count)) = struct_hfa(&f.ret, prog) {
                for _ in 0..count {
                    sig.returns.push(AbiParam::new(elem_ct));
                }
                return Ok(sig);
            }
            if let Some(chunks) = struct_chunks(&f.ret, prog) {
                for _ in 0..chunks {
                    sig.returns.push(AbiParam::new(types::I64));
                }
                return Ok(sig);
            }
        }
        let ret = mir_to_clif(&f.ret)
            .ok_or(CompileError::Unsupported("unit return through ABI"))?;
        sig.returns.push(AbiParam::new(ret));
    }
    Ok(sig)
}

/// For an `@extern(C)` CRepr struct ≤ 16 B: returns `Some(chunks)`
/// where `chunks` is 1 or 2 i64 GPR slots. > 16 B / non-CRepr / non-
/// Object types return `None` (caller treats as pointer-sized i64).
fn struct_chunks(ty: &MirTy, prog: &Program) -> Option<usize> {
    if let MirTy::Object(cid) = ty {
        let layout = &prog.classes[cid.0 as usize];
        if matches!(
            layout.repr,
            ilang_mir::ClassRepr::CRepr | ilang_mir::ClassRepr::CPacked
        ) {
            if layout.c_size <= 8 {
                return Some(1);
            }
            if layout.c_size <= 16 {
                return Some(2);
            }
        }
    }
    None
}

/// HFA detection (AArch64 AAPCS64 / x86_64 SysV "homogeneous
/// floating-point aggregate"): 1–4 fields, all the same float type.
/// Returns `Some((elem_clif_type, count))` so the caller can push a
/// matching `AbiParam(F32|F64)` per element.
fn struct_hfa(ty: &MirTy, prog: &Program) -> Option<(cranelift::prelude::Type, usize)> {
    use cranelift::prelude::types as ct;
    if let MirTy::Object(cid) = ty {
        let layout = &prog.classes[cid.0 as usize];
        if !matches!(layout.repr, ilang_mir::ClassRepr::CRepr) {
            return None;
        }
        if layout.fields.is_empty() || layout.fields.len() > 4 {
            return None;
        }
        let mut clif_ty: Option<cranelift::prelude::Type> = None;
        for f in &layout.fields {
            let ct_for = match &f.ty {
                MirTy::F32 => ct::F32,
                MirTy::F64 => ct::F64,
                _ => return None,
            };
            match clif_ty {
                None => clif_ty = Some(ct_for),
                Some(prev) if prev != ct_for => return None,
                _ => {}
            }
        }
        return clif_ty.map(|c| (c, layout.fields.len()));
    }
    None
}

/// Larger CRepr structs (> 16 B) are returned through a hidden
/// pointer (`ArgumentPurpose::StructReturn`). Returns `Some(c_size)`
/// for those, `None` for chunkable / non-CRepr / non-Object types.
fn struct_indirect(ty: &MirTy, prog: &Program) -> Option<i64> {
    if let MirTy::Object(cid) = ty {
        let layout = &prog.classes[cid.0 as usize];
        if matches!(
            layout.repr,
            ilang_mir::ClassRepr::CRepr | ilang_mir::ClassRepr::CPacked
        ) && layout.c_size > 16
        {
            return Some(layout.c_size);
        }
    }
    None
}

fn lower_function(
    fb: &mut ClifFnBuilder,
    func: &MirFunction,
    fn_ids: &HashMap<FuncId, cranelift_module::FuncId>,
    _fn_sigs: &HashMap<FuncId, Signature>,
    builtin_ids: &HashMap<String, (cranelift_module::FuncId, Signature)>,
    static_data: &HashMap<StaticSlotId, DataId>,
    string_data: &HashMap<Symbol, DataId>,
    alloc_id: cranelift_module::FuncId,
    map_ids: MapIds,
    str_ids: StrIds,
    print_ids: PrintIds,
    panic_aux: PanicAux,
    print_lits: PrintLits,
    module: &mut JITModule,
    prog: &Program,
    class_global: &[u32],
    enum_global: &[u32],
) -> Result<(), CompileError> {
    // Allocate clif blocks 1:1 with MIR blocks. Skip Unit-typed
    // block params at the clif level since clif has no unit type;
    // the matching ValueIds get a sentinel (i8 0) at use-sites.
    let mut blocks: Vec<cranelift::prelude::Block> = Vec::with_capacity(func.blocks.len());
    for (i, blk) in func.blocks.iter().enumerate() {
        let b = fb.create_block();
        for &p in &blk.params {
            let pty = func.ty_of(p);
            if let Some(ct) = mir_to_clif(pty) {
                fb.append_block_param(b, ct);
            }
        }
        // The entry block carries the hidden env-pointer param last
        // (matching the unified clif signature in `clif_signature_for`).
        if i == func.entry.0 as usize {
            fb.append_block_param(b, types::I64);
        }
        blocks.push(b);
    }

    // Map ValueId → cranelift Value. Unit-typed values aren't bound
    // (use-sites filter them out).
    let mut vmap: HashMap<ValueId, Value> = HashMap::new();
    for (i, blk) in func.blocks.iter().enumerate() {
        let cb = blocks[i];
        let mut clif_idx = 0;
        for &p in &blk.params {
            if mir_to_clif(func.ty_of(p)).is_some() {
                let cv = fb.block_params(cb)[clif_idx];
                vmap.insert(p, cv);
                clif_idx += 1;
            }
        }
    }

    let entry_clif = blocks[func.entry.0 as usize];
    fb.switch_to_block(entry_clif);
    fb.seal_block(entry_clif);

    // The hidden env-ptr is the entry block's last clif param.
    let env_value: Value = {
        let bps = fb.block_params(entry_clif);
        bps[bps.len() - 1]
    };

    // Declare a Cranelift `Variable` for every MIR local. Cranelift
    // performs the on-demand SSA construction (block-arg insertion
    // for loop-carried values) once we use def_var / use_var.
    let mut locals: Vec<Variable> = Vec::with_capacity(func.local_tys.len());
    for lt in func.local_tys.iter() {
        let ct = mir_to_clif(lt).unwrap_or(types::I64);
        let var = fb.declare_var(ct);
        locals.push(var);
    }

    // Lower in MIR-block order. Any MIR block reachable from terminators
    // is sealed after we've emitted its branch sources — for the M1
    // subset (no irreducible CFG), it's safe to seal each block right
    // after emitting all its predecessors. We seal aggressively at the
    // end to cover the common case.
    for (i, blk) in func.blocks.iter().enumerate() {
        let cb = blocks[i];
        if i != func.entry.0 as usize {
            fb.switch_to_block(cb);
        }
        for inst in &blk.insts {
            lower_inst(
                fb,
                inst,
                &mut vmap,
                func,
                fn_ids,
                builtin_ids,
                static_data,
                string_data,
                alloc_id,
                map_ids,
                str_ids,
                print_ids,
                panic_aux,
                print_lits,
                module,
                &locals,
                prog,
                env_value,
                class_global,
                enum_global,
            )?;
        }
        lower_term(fb, &blk.term, &vmap, &blocks)?;
    }
    // Seal all blocks (M1 doesn't construct cycles via ssa add_predecessor;
    // every predecessor is already known by structure).
    for (i, _) in func.blocks.iter().enumerate() {
        if i != func.entry.0 as usize {
            fb.seal_block(blocks[i]);
        }
    }
    Ok(())
}

fn lower_inst(
    fb: &mut ClifFnBuilder,
    inst: &Inst,
    vmap: &mut HashMap<ValueId, Value>,
    func: &MirFunction,
    fn_ids: &HashMap<FuncId, cranelift_module::FuncId>,
    builtin_ids: &HashMap<String, (cranelift_module::FuncId, Signature)>,
    static_data: &HashMap<StaticSlotId, DataId>,
    string_data: &HashMap<Symbol, DataId>,
    alloc_id: cranelift_module::FuncId,
    map_ids: MapIds,
    str_ids: StrIds,
    print_ids: PrintIds,
    panic_aux: PanicAux,
    print_lits: PrintLits,
    module: &mut JITModule,
    locals: &[Variable],
    prog: &Program,
    env_value: Value,
    class_global: &[u32],
    enum_global: &[u32],
) -> Result<(), CompileError> {
    match inst {
        Inst::Const { dst, value } => {
            let ty = func.ty_of(*dst);
            if matches!(ty, MirTy::Unit) || matches!(value, MirConst::Unit) {
                return Ok(());
            }
            // String consts go through Cranelift `symbol_value` to get
            // the data symbol's runtime address.
            if let MirConst::Str(s) = value {
                let did = *string_data.get(s).ok_or_else(|| {
                    CompileError::Other(format!("missing string data for {:?}", s.as_str()))
                })?;
                let gv = module.declare_data_in_func(did, fb.func);
                let base = fb.ins().symbol_value(types::I64, gv);
                // The user-visible string pointer skips the 8-byte
                // length prefix (see string_data layout above).
                let off = fb.ins().iconst(types::I64, 8);
                let v = fb.ins().iadd(base, off);
                vmap.insert(*dst, v);
                return Ok(());
            }
            let cv = lower_const(fb, value, ty)?;
            vmap.insert(*dst, cv);
        }
        Inst::BinOp { dst, op, lhs, rhs } => {
            let lv = vmap[lhs];
            let rv = vmap[rhs];
            // Runtime div/0 / mod/0 check on int division.
            if matches!(
                op,
                BinOp::IDivS | BinOp::IDivU | BinOp::IRemS | BinOp::IRemU
            ) {
                let rv_ty = fb.func.dfg.value_type(rv);
                let zero = fb.ins().iconst(rv_ty, 0);
                let is_zero = fb.ins().icmp(IntCC::Equal, rv, zero);
                let msg = if matches!(op, BinOp::IRemS | BinOp::IRemU) {
                    panic_aux.msg_mod
                } else {
                    panic_aux.msg_div
                };
                emit_panic_if(fb, module, panic_aux.fn_id, msg, is_zero);
            }
            let v = match op {
                BinOp::StrConcat => {
                    let r = module.declare_func_in_func(str_ids.concat, fb.func);
                    let call = fb.ins().call(r, &[lv, rv]);
                    fb.inst_results(call)[0]
                }
                BinOp::StrEq => {
                    let r = module.declare_func_in_func(str_ids.eq, fb.func);
                    let call = fb.ins().call(r, &[lv, rv]);
                    let raw = fb.inst_results(call)[0];
                    fb.ins().ireduce(types::I8, raw)
                }
                BinOp::StrNe => {
                    let r = module.declare_func_in_func(str_ids.eq, fb.func);
                    let call = fb.ins().call(r, &[lv, rv]);
                    let raw = fb.inst_results(call)[0];
                    let lo = fb.ins().ireduce(types::I8, raw);
                    let one = fb.ins().iconst(types::I8, 1);
                    fb.ins().bxor(lo, one)
                }
                _ => lower_binop(fb, *op, lv, rv),
            };
            vmap.insert(*dst, v);
        }
        Inst::UnOp { dst, op, src } => {
            let sv = vmap[src];
            let v = match op {
                UnOp::INeg => fb.ins().ineg(sv),
                UnOp::FNeg => fb.ins().fneg(sv),
                UnOp::Not => fb.ins().bnot(sv),
                UnOp::BoolNot => {
                    let zero = fb.ins().iconst(types::I8, 0);
                    fb.ins().icmp(IntCC::Equal, sv, zero)
                }
            };
            vmap.insert(*dst, v);
        }
        Inst::Cast { dst, kind, src } => {
            let sv = vmap[src];
            let dst_ty = func.ty_of(*dst);
            let src_ty = func.ty_of(*src);
            let v = lower_cast(fb, *kind, sv, dst_ty, src_ty)?;
            vmap.insert(*dst, v);
        }
        Inst::Call { dst, callee, args } => {
            // `console.log(...)` — special-cased variadic. Each
            // argument prints with a per-type host helper, separated
            // by spaces and terminated by a newline.
            if let FuncRef::Builtin(sym) = callee {
                if sym.as_str() == "console_log" {
                    // Skip Unit-typed args entirely. The CLI's
                    // `wrap_trailing_print` may pass them when a
                    // program's trailing expression is a void method
                    // call (e.g. `test.expect(...)`); in that case
                    // nothing should be printed and the trailing
                    // newline is suppressed too so stdout stays clean.
                    let mut printed = 0usize;
                    for a in args.iter() {
                        let aty = func.ty_of(*a).clone();
                        if matches!(aty, MirTy::Unit) {
                            continue;
                        }
                        if printed > 0 {
                            let r = module.declare_func_in_func(print_ids.space, fb.func);
                            fb.ins().call(r, &[]);
                        }
                        let av = vmap[a];
                        emit_print_value(fb, module, print_ids, print_lits, &aty, av, enum_global);
                        printed += 1;
                    }
                    if printed > 0 {
                        let r = module.declare_func_in_func(print_ids.newline, fb.func);
                        fb.ins().call(r, &[]);
                    }
                    if let Some(d) = dst {
                        // console.log returns Unit — produce a sentinel
                        // for any (unlikely) consumer.
                        let sentinel = fb.ins().iconst(types::I8, 0);
                        vmap.insert(*d, sentinel);
                    }
                    return Ok(());
                }
            }
            let mut arg_vs: Vec<Value> = Vec::with_capacity(args.len());
            // Resolve callee FuncId early so we can know whether it's
            // extern (and split CRepr struct args into chunks).
            let (callee_cid, is_callee_extern, is_callee_builtin) = match callee {
                FuncRef::Local(id) => {
                    let target_func = &prog.functions[id.0 as usize];
                    let is_extern_callee =
                        matches!(target_func.kind, ilang_mir::FunctionKind::Extern { .. });
                    let cid = *fn_ids.get(id).ok_or_else(|| {
                        CompileError::Other(format!("missing fn id #{}", id.0))
                    })?;
                    (Some(cid), is_extern_callee, false)
                }
                _ => (None, false, false),
            };
            // sret: pre-alloc the destination struct and pass its
            // pointer as the hidden first arg.
            let sret_dst = if is_callee_extern {
                if let Some(d) = dst {
                    let dst_ty = func.ty_of(*d).clone();
                    if let Some(c_size) = struct_indirect(&dst_ty, prog) {
                        let size_v = fb.ins().iconst(types::I64, c_size);
                        let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
                        let alloc_call = fb.ins().call(alloc_ref, &[size_v]);
                        let ptr = fb.inst_results(alloc_call)[0];
                        arg_vs.push(ptr);
                        Some((*d, ptr))
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };
            // Builtins like array_map / array_filter / array_for_each /
            // array_slice / array_index_of / array_includes consume a
            // dynamic-array header (6×i64 [len|cap|data|rc|kind|stride]).
            // Fixed-length arrays carry no header — they're just inline
            // element data — so wrap them on-the-fly via __fixed_to_dyn
            // so the receiver sees a uniform header shape.
            let wrap_fixed_first_arg: Option<i64> = if let FuncRef::Builtin(sym) = callee {
                let kind_tag = match sym.as_str() {
                    "array_map"
                    | "array_filter"
                    | "array_for_each"
                    | "array_slice"
                    | "array_index_of"
                    | "array_includes" => Some(0i64),
                    _ => None,
                };
                kind_tag
            } else {
                None
            };
            for (arg_ix, a) in args.iter().enumerate() {
                let mut av = *vmap.get(a).unwrap_or_else(|| {
                    panic!(
                        "missing vmap entry for arg {:?} in call to {:?}",
                        a, callee
                    )
                });
                if let (Some(kind_tag_for_obj), 0) = (wrap_fixed_first_arg, arg_ix) {
                    if let MirTy::Array { elem, len: Some(n) } = func.ty_of(*a) {
                        let stride = elem_byte_stride(elem);
                        let kind_tag = if matches!(**elem, MirTy::Object(_)) {
                            1
                        } else {
                            kind_tag_for_obj
                        };
                        let len_v = fb.ins().iconst(types::I64, *n as i64);
                        let stride_v = fb.ins().iconst(types::I64, stride);
                        let kind_v = fb.ins().iconst(types::I64, kind_tag);
                        let f = module.declare_func_in_func(str_ids.fixed_to_dyn, fb.func);
                        let call = fb.ins().call(f, &[av, len_v, stride_v, kind_v]);
                        av = fb.inst_results(call)[0];
                    }
                }
                if is_callee_extern {
                    let aty = func.ty_of(*a);
                    if let Some((elem_ct, count)) = struct_hfa(aty, prog) {
                        // Read `count` floats from the struct body
                        // (offset = i × elem_byte_size).
                        let elem_size: i32 = if elem_ct == types::F32 { 4 } else { 8 };
                        for c in 0..count {
                            let v = fb.ins().load(
                                elem_ct,
                                MemFlags::trusted(),
                                av,
                                (c as i32) * elem_size,
                            );
                            arg_vs.push(v);
                        }
                        continue;
                    }
                    if let Some(chunks) = struct_chunks(aty, prog) {
                        for c in 0..chunks {
                            let cell = fb.ins().load(
                                types::I64,
                                MemFlags::trusted(),
                                av,
                                (c as i32) * 8,
                            );
                            arg_vs.push(cell);
                        }
                        continue;
                    }
                }
                arg_vs.push(av);
            }
            let _ = is_callee_builtin;
            let (cid, is_builtin) = match callee {
                FuncRef::Local(_) => (callee_cid.unwrap(), is_callee_extern),
                FuncRef::Builtin(sym) => {
                    // FFI marshalling helpers are declared by name —
                    // route them via `module.declarations` lookup so we
                    // don't need a separate id table.
                    if matches!(
                        sym.as_str(),
                        "cstrFromString"
                            | "stringFromCstr"
                            | "cstrArrayToStrings"
                            | "__array_data_ptr"
                            | "__enum_box"
                            | "__c_array_to_array"
                            | "__repl_load_slot"
                            | "__repl_store_slot"
                            | "__read_i8" | "__read_i16" | "__read_i32" | "__read_i64"
                            | "__read_u8" | "__read_u16" | "__read_u32" | "__read_u64"
                            | "__read_f32" | "__read_f64"
                            | "__write_i8" | "__write_i16" | "__write_i32" | "__write_i64"
                            | "__write_u8" | "__write_u16" | "__write_u32" | "__write_u64"
                            | "__write_f32" | "__write_f64"
                            | "freeCstr"
                            | "errnoCheck"
                            | "errnoCheckI64"
                            | "os.errno"
                            | "os.setErrno"
                    ) {
                        let cid = module
                            .declarations()
                            .get_name(sym.as_str())
                            .and_then(|n| match n {
                                cranelift_module::FuncOrDataId::Func(id) => Some(id),
                                _ => None,
                            })
                            .ok_or_else(|| {
                                CompileError::Other(format!(
                                    "ffi helper `{sym}` not declared"
                                ))
                            })?;
                        (cid, true)
                    } else {
                    // Translate well-known MIR builtin names to the
                    // host-registered Cranelift FuncIds.
                    let host_id = match sym.as_str() {
                        "str_length" => Some(str_ids.length),
                        "str_concat" => Some(str_ids.concat),
                        "str_eq" => Some(str_ids.eq),
                        "int_to_string" => Some(str_ids.int_to_string),
                        "bool_to_string" => Some(str_ids.bool_to_string),
                        "str_to_upper" => Some(str_ids.to_upper),
                        "str_to_lower" => Some(str_ids.to_lower),
                        "str_trim" => Some(str_ids.trim),
                        "str_includes" => Some(str_ids.includes),
                        "str_starts_with" => Some(str_ids.starts_with),
                        "str_ends_with" => Some(str_ids.ends_with),
                        "str_char_at" => Some(str_ids.char_at),
                        "str_slice" => Some(str_ids.slice),
                        "str_replace" => Some(str_ids.replace),
                        "array_index_of" => Some(str_ids.array_index_of),
                        "array_includes" => Some(str_ids.array_includes),
                        "array_push" => Some(str_ids.array_push),
                        "array_pop" => Some(str_ids.array_pop),
                        "array_map" => Some(str_ids.array_map),
                        "array_filter" => Some(str_ids.array_filter),
                        "array_for_each" => Some(str_ids.array_for_each),
                        "array_slice" => Some(str_ids.array_slice),
                        "str_split" => Some(str_ids.str_split),
                        "map_get" => Some(map_ids.get),
                        "map_get_optional" => Some(map_ids.get_optional),
                        "map_set" => Some(map_ids.set),
                        "map_size" => Some(map_ids.size),
                        "map_has" => Some(map_ids.has),
                        "map_delete" => Some(map_ids.delete),
                        "map_keys" => Some(map_ids.keys),
                        "map_values" => Some(map_ids.values),
                        "class_name" => Some(panic_aux.class_name),
                        _ => None,
                    };
                    let cid = match host_id {
                        Some(id) => id,
                        None => {
                            builtin_ids
                                .get(sym.as_str())
                                .ok_or_else(|| {
                                    CompileError::Other(format!(
                                        "unregistered builtin `{sym}`"
                                    ))
                                })?
                                .0
                        }
                    };
                    (cid, true)
                    }
                }
                FuncRef::Extern { .. } => {
                    return Err(CompileError::Unsupported("extern call"));
                }
            };
            // Local fns carry the unified env-trailing param; builtins
            // don't.
            if !is_builtin {
                let zero = fb.ins().iconst(types::I64, 0);
                arg_vs.push(zero);
            }
            // For builtins like the map / array / str runtime, the
            // declared sig is uniformly i64. Auto-extend any narrower
            // arg so the verifier doesn't complain (bool/i32/f64
            // bitcast to i64). Signed MIR ints sign-extend; unsigned
            // / bool / raw bit patterns zero-extend. Without the
            // signed branch, e.g. `(-1: i32).toString()` would pass
            // `4294967295` to `__int_to_string` and display the
            // unsigned bit pattern instead of `-1` (mirrored across
            // i8 / i16 / i32 — see int_to_string_signed.il).
            if is_builtin {
                let sig_params = module.declarations()
                    .get_function_decl(cid)
                    .signature
                    .params
                    .clone();
                for (i, av) in arg_vs.iter_mut().enumerate() {
                    let want = match sig_params.get(i) {
                        Some(p) => p.value_type,
                        None => continue,
                    };
                    let got = fb.func.dfg.value_type(*av);
                    if got == want {
                        continue;
                    }
                    if want == types::I64 {
                        if got == types::F64 {
                            *av = fb.ins().bitcast(types::I64, MemFlags::new(), *av);
                        } else if got == types::F32 {
                            let r32 = fb.ins().bitcast(types::I32, MemFlags::new(), *av);
                            *av = fb.ins().uextend(types::I64, r32);
                        } else if got.is_int() && got.bits() < 64 {
                            // arg_vs is index-aligned with `args` for
                            // builtin calls (no sret prefix, no trailing
                            // env). Look up the MIR type to choose the
                            // sign-correct widening.
                            let signed = args
                                .get(i)
                                .map(|a| func.ty_of(*a).is_signed_int())
                                .unwrap_or(false);
                            *av = if signed {
                                fb.ins().sextend(types::I64, *av)
                            } else {
                                fb.ins().uextend(types::I64, *av)
                            };
                        }
                    }
                }
            }
            let local_ref = module.declare_func_in_func(cid, fb.func);
            // C-variadic extern: build a per-call signature with the
            // actual arg types and dispatch via call_indirect (the
            // declared signature only covers the fixed prefix). On
            // Apple AArch64 the variadic ABI pads the integer / FP
            // register files so the variadic tail spills to the stack
            // — fill the spare slots with zero placeholders.
            let variadic_dispatch = if is_callee_extern {
                if let FuncRef::Local(fid) = callee {
                    let target = &prog.functions[fid.0 as usize];
                    if target.is_variadic && arg_vs.len() > target.params.len() {
                        Some(target.params.len())
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };
            let inst_ref = if let Some(n_fixed) = variadic_dispatch {
                let mut cl_sig = module.make_signature();
                let needs_apple_pad =
                    cfg!(target_os = "macos") && cfg!(target_arch = "aarch64");
                let fixed: Vec<Value> = arg_vs[..n_fixed].to_vec();
                let varargs: Vec<Value> = arg_vs[n_fixed..].to_vec();
                for v in &fixed {
                    cl_sig.params.push(AbiParam::new(fb.func.dfg.value_type(*v)));
                }
                let mut padded: Vec<Value> = fixed.clone();
                if needs_apple_pad && !varargs.is_empty() {
                    let n_int_fixed = fixed
                        .iter()
                        .filter(|v| fb.func.dfg.value_type(**v).is_int())
                        .count();
                    let n_fp_fixed = fixed
                        .iter()
                        .filter(|v| fb.func.dfg.value_type(**v).is_float())
                        .count();
                    let n_int_pad = 8usize.saturating_sub(n_int_fixed);
                    let n_fp_pad = 8usize.saturating_sub(n_fp_fixed);
                    for _ in 0..n_int_pad {
                        cl_sig.params.push(AbiParam::new(types::I64));
                    }
                    for _ in 0..n_fp_pad {
                        cl_sig.params.push(AbiParam::new(types::F64));
                    }
                    let zero_i = fb.ins().iconst(types::I64, 0);
                    let zero_f = fb.ins().f64const(0.0);
                    for _ in 0..n_int_pad {
                        padded.push(zero_i);
                    }
                    for _ in 0..n_fp_pad {
                        padded.push(zero_f);
                    }
                }
                for v in &varargs {
                    cl_sig.params.push(AbiParam::new(fb.func.dfg.value_type(*v)));
                    padded.push(*v);
                }
                let target_func = match callee {
                    FuncRef::Local(fid) => &prog.functions[fid.0 as usize],
                    _ => unreachable!(),
                };
                if !matches!(target_func.ret, MirTy::Unit) {
                    if let Some(rt) = elem_clif_type(&target_func.ret) {
                        cl_sig.returns.push(AbiParam::new(rt));
                    } else {
                        cl_sig.returns.push(AbiParam::new(types::I64));
                    }
                }
                let sig_ref = fb.import_signature(cl_sig);
                let func_addr = fb.ins().func_addr(types::I64, local_ref);
                fb.ins().call_indirect(sig_ref, func_addr, &padded)
            } else {
                fb.ins().call(local_ref, &arg_vs)
            };
            // sret: the call has no clif return; the pre-alloc'd
            // pointer is what the user sees.
            if let Some((d, ptr)) = sret_dst {
                vmap.insert(d, ptr);
                return Ok(());
            }
            if let Some(d) = dst {
                let dst_ty = func.ty_of(*d).clone();
                if is_callee_extern {
                    if let Some((elem_ct, count)) = struct_hfa(&dst_ty, prog) {
                        let layout = if let MirTy::Object(cid) = &dst_ty {
                            &prog.classes[cid.0 as usize]
                        } else {
                            unreachable!()
                        };
                        let size_v = fb.ins().iconst(types::I64, layout.c_size.max(1));
                        let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
                        let alloc_call = fb.ins().call(alloc_ref, &[size_v]);
                        let ptr = fb.inst_results(alloc_call)[0];
                        let results: Vec<Value> = fb.inst_results(inst_ref).to_vec();
                        let elem_size: i32 = if elem_ct == types::F32 { 4 } else { 8 };
                        for (i, &v) in results.iter().take(count).enumerate() {
                            fb.ins().store(
                                MemFlags::trusted(),
                                v,
                                ptr,
                                (i as i32) * elem_size,
                            );
                        }
                        vmap.insert(*d, ptr);
                        return Ok(());
                    }
                    if let Some(chunks) = struct_chunks(&dst_ty, prog) {
                        let layout = if let MirTy::Object(cid) = &dst_ty {
                            &prog.classes[cid.0 as usize]
                        } else {
                            unreachable!()
                        };
                        let size_v = fb.ins().iconst(types::I64, layout.c_size.max(1));
                        let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
                        let alloc_call = fb.ins().call(alloc_ref, &[size_v]);
                        let ptr = fb.inst_results(alloc_call)[0];
                        let results: Vec<Value> = fb.inst_results(inst_ref).to_vec();
                        for (i, &chunk) in results.iter().take(chunks).enumerate() {
                            fb.ins().store(
                                MemFlags::trusted(),
                                chunk,
                                ptr,
                                (i as i32) * 8,
                            );
                        }
                        vmap.insert(*d, ptr);
                        return Ok(());
                    }
                }
                let results = fb.inst_results(inst_ref);
                if let Some(&v) = results.first() {
                    let v_clif = fb.func.dfg.value_type(v);
                    let want = mir_to_clif(&dst_ty);
                    let v_adj = match (want, v_clif) {
                        (Some(target), got) if target.bits() < got.bits() => {
                            fb.ins().ireduce(target, v)
                        }
                        _ => v,
                    };
                    vmap.insert(*d, v_adj);
                }
            }
        }
        Inst::VirtCall { dst, recv, slot, args } => {
            // Load class_id from object header, dispatch via the
            // host runtime helper, then call_indirect.
            let recv_v = vmap[recv];
            let cid = fb.ins().load(types::I64, MemFlags::trusted(), recv_v, 0);
            let slot_v = fb.ins().iconst(types::I64, slot.0 as i64);
            let dispatch_ref = module.declare_func_in_func(str_ids.virt_dispatch, fb.func);
            let lookup = fb.ins().call(dispatch_ref, &[cid, slot_v]);
            let fn_ptr = fb.inst_results(lookup)[0];
            // Build a clif sig matching the method ABI: this + args + env.
            let mut clif_sig = module.make_signature();
            clif_sig.params.push(AbiParam::new(types::I64));
            // Other params: re-derive from the receiver's class
            // method's MIR sig. For simplicity treat each arg's clif
            // type as its current value's type at the call site.
            for a in args.iter() {
                let ty = fb.func.dfg.value_type(vmap[a]);
                clif_sig.params.push(AbiParam::new(ty));
            }
            clif_sig.params.push(AbiParam::new(types::I64)); // env
            let dst_ty_mir = dst.map(|d| func.ty_of(d).clone());
            if let Some(t) = &dst_ty_mir {
                if !matches!(t, MirTy::Unit) {
                    if let Some(ct) = mir_to_clif(t) {
                        clif_sig.returns.push(AbiParam::new(ct));
                    }
                }
            }
            let sig_ref = fb.import_signature(clif_sig);
            let mut arg_vs: Vec<Value> = vec![recv_v];
            for a in args.iter() {
                arg_vs.push(vmap[a]);
            }
            let zero = fb.ins().iconst(types::I64, 0);
            arg_vs.push(zero);
            let inst_ref = fb.ins().call_indirect(sig_ref, fn_ptr, &arg_vs);
            if let Some(d) = dst {
                let results = fb.inst_results(inst_ref);
                if let Some(&v) = results.first() {
                    vmap.insert(*d, v);
                }
            }
        }
        Inst::CallIndirect { dst, callee, sig, args } => {
            // Closure value: pointer to `[fn_ptr | captures...]`.
            let closure = vmap[callee];
            let fn_ptr = fb.ins().load(types::I64, MemFlags::trusted(), closure, 0);
            // Build an indirect signature: user params + trailing env.
            let mut clif_sig = module.make_signature();
            for p in sig.params.iter() {
                if let Some(ct) = mir_to_clif(p) {
                    clif_sig.params.push(AbiParam::new(ct));
                }
            }
            clif_sig.params.push(AbiParam::new(types::I64));
            if !matches!(sig.ret, MirTy::Unit) {
                if let Some(ct) = mir_to_clif(&sig.ret) {
                    clif_sig.returns.push(AbiParam::new(ct));
                }
            }
            let sig_ref = fb.import_signature(clif_sig);
            let mut arg_vs: Vec<Value> = args.iter().map(|a| vmap[a]).collect();
            arg_vs.push(closure); // env_ptr = closure block ptr
            let inst_ref = fb.ins().call_indirect(sig_ref, fn_ptr, &arg_vs);
            if let Some(d) = dst {
                let results = fb.inst_results(inst_ref);
                if let Some(&v) = results.first() {
                    vmap.insert(*d, v);
                }
            }
        }
        Inst::MakeClosure { dst, func: fid, captures } => {
            let cid = *fn_ids.get(fid).ok_or_else(|| {
                CompileError::Other(format!("missing fn id #{}", fid.0))
            })?;
            let local_ref = module.declare_func_in_func(cid, fb.func);
            let n_caps = captures.len() as i64;
            // Layout: [fn_ptr @ 0 | rc @ 8 | capture_0 @ 16 | ...]
            let bytes = fb.ins().iconst(types::I64, (2 + n_caps) * 8);
            let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
            let call = fb.ins().call(alloc_ref, &[bytes]);
            let ptr = fb.inst_results(call)[0];
            let fn_addr = fb.ins().func_addr(types::I64, local_ref);
            fb.ins().store(MemFlags::trusted(), fn_addr, ptr, 0);
            let one = fb.ins().iconst(types::I64, 1);
            fb.ins().store(MemFlags::trusted(), one, ptr, 8);
            for (i, c) in captures.iter().enumerate() {
                let v_ext = extend_to_i64(fb, vmap[c]);
                fb.ins().store(
                    MemFlags::trusted(),
                    v_ext,
                    ptr,
                    16 + (i as i32) * 8,
                );
            }
            vmap.insert(*dst, ptr);
        }
        Inst::LoadCapture { dst, idx } => {
            // Captures live at `env + 16 + idx*8`; env is the closure
            // block pointer (the trailing hidden param).
            let off = 16 + (*idx as i32) * 8;
            let raw = fb.ins().load(types::I64, MemFlags::trusted(), env_value, off);
            let dst_ty = func.ty_of(*dst).clone();
            let v = reduce_from_i64(fb, &dst_ty, raw);
            vmap.insert(*dst, v);
        }
        // ARC operations are stubbed in M1: refcount machinery
        // arrives once the runtime is wired. Treating them as no-ops
        // means programs leak heap allocations until then, which is
        // acceptable for short-running test programs.
        Inst::Release { value } => {
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
                        // CRepr struct: no rc header, free the
                        // backing buffer directly. The lower side
                        // only emits this Release for Locals
                        // tagged in `crepr_owned_locals` — i.e.
                        // values that came from a fresh NewObject
                        // (or an aggregate-literal desugar that
                        // owns its temp), never a `let p =
                        // r.origin` borrow.
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
                MirTy::Fn(_) => {
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.release_closure, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Array { len, .. } => {
                    if len.is_some() {
                        return Ok(());
                    }
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.release_array, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Optional(_) => {
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.release_optional, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Tuple(_) => {
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.release_tuple, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Map { .. } => {
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.release_map, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Str => {
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.release_string, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Enum(_) => {
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.release_enum, fb.func);
                    fb.ins().call(r, &[av]);
                }
                _ => {}
            }
        }
        Inst::Retain { value } => {
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
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.retain_obj, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Fn(_) => {
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.retain_closure, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Array { len, .. } => {
                    if len.is_some() {
                        return Ok(());
                    }
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.retain_array, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Optional(_) => {
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.retain_optional, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Tuple(_) => {
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.retain_tuple, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Map { .. } => {
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.retain_map, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Str => {
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.retain_string, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Enum(_) => {
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.retain_enum, fb.func);
                    fb.ins().call(r, &[av]);
                }
                _ => {}
            }
        }
        Inst::WeakRetain { .. }
        | Inst::WeakRelease { .. } => {}
        Inst::TypeOf { dst, value } => {
            // Return the dynamic class id (i64) — used as an opaque
            // `Type` handle. Full `Type` API arrives with the runtime.
            let p = vmap[value];
            let cid = fb.ins().load(types::I64, MemFlags::trusted(), p, 0);
            vmap.insert(*dst, cid);
        }
        Inst::IsInstance { dst, value, class } => {
            let p = vmap[value];
            let cid = fb.ins().load(types::I64, MemFlags::trusted(), p, 0);
            let v = emit_is_subclass(fb, cid, *class, prog, class_global);
            vmap.insert(*dst, v);
        }
        Inst::DowncastOrNone { dst, value, class } => {
            // `value as? Class` → some(value) if dynamic class is
            // a subtype of `class`, else none. Optional<Object> is
            // boxed: we emit NewOptional on the some-branch, 0 on the
            // none-branch, and merge through a block-arg.
            let p = vmap[value];
            let cid = fb.ins().load(types::I64, MemFlags::trusted(), p, 0);
            let cond = emit_is_subclass(fb, cid, *class, prog, class_global);

            let some_blk = fb.create_block();
            let none_blk = fb.create_block();
            let cont_blk = fb.create_block();
            let result = fb.append_block_param(cont_blk, types::I64);

            fb.ins().brif(cond, some_blk, &[], none_blk, &[]);

            fb.switch_to_block(some_blk);
            fb.seal_block(some_blk);
            // Allocate one i64 cell containing the value.
            let bytes = fb.ins().iconst(types::I64, 8);
            let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
            let call = fb.ins().call(alloc_ref, &[bytes]);
            let ptr = fb.inst_results(call)[0];
            fb.ins().store(MemFlags::trusted(), p, ptr, 0);
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
            let rc = fb.ins().load(types::I64, MemFlags::trusted(), p, 8);
            let alive = fb.ins().icmp_imm(IntCC::SignedGreaterThan, rc, 0);
            let alloc_blk = fb.create_block();
            fb.ins().brif(alive, alloc_blk, &[], none_blk, &[]);

            // alive: bump strong rc (caller now owns +1) and box into
            // a fresh Optional cell.
            fb.switch_to_block(alloc_blk);
            fb.seal_block(alloc_blk);
            let one = fb.ins().iconst(types::I64, 1);
            let new_rc = fb.ins().iadd(rc, one);
            fb.ins().store(MemFlags::trusted(), new_rc, p, 8);
            let bytes = fb.ins().iconst(types::I64, 24);
            let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
            let call = fb.ins().call(alloc_ref, &[bytes]);
            let cell = fb.inst_results(call)[0];
            fb.ins().store(MemFlags::trusted(), p, cell, 0);
            fb.ins().store(MemFlags::trusted(), one, cell, 8);
            let kind = fb.ins().iconst(types::I64, 1); // PrintKind::Object cascade
            fb.ins().store(MemFlags::trusted(), kind, cell, 16);
            fb.ins().jump(cont, [cell.into()].iter());

            fb.switch_to_block(none_blk);
            fb.seal_block(none_blk);
            fb.ins().jump(cont, [zero.into()].iter());

            fb.switch_to_block(cont);
            fb.seal_block(cont);
            let v = fb.block_params(cont)[0];
            vmap.insert(*dst, v);
        }
        Inst::DefLocal { local, value } => {
            let var = locals[local.0 as usize];
            let v = vmap[value];
            if std::env::var("ILANG_DEBUG_DEFLOCAL").is_ok() {
                let want = func.local_tys[local.0 as usize].clone();
                let got = fb.func.dfg.value_type(v);
                eprintln!(
                    "[deflocal] fn={} local#{} declared={want} clif_val_ty={got}",
                    func.name.as_str(), local.0
                );
            }
            fb.def_var(var, v);
        }
        Inst::UseLocal { dst, local } => {
            let var = locals[local.0 as usize];
            let v = fb.use_var(var);
            vmap.insert(*dst, v);
        }
        Inst::NewObject { dst, class, init_args, init } => {
            let layout = &prog.classes[class.0 as usize];
            // `@extern(C) struct` lives flat with no header / rc:
            // alloc exactly c_size bytes (zero-init by host_mir_alloc)
            // and bind that pointer. No init / deinit / vtable.
            if matches!(
                layout.repr,
                ilang_mir::ClassRepr::CRepr
                    | ilang_mir::ClassRepr::CPacked
                    | ilang_mir::ClassRepr::CUnion
            ) {
                // CRepr struct alloc. With a flexible array tail
                // (`new packet(n)`) the user passes the FAM length
                // as the first arg; total size = c_size +
                // n*flex_elem_size.
                let size_v = if layout.flex_elem_size > 0 && !init_args.is_empty() {
                    let n_v = vmap[&init_args[0]];
                    let n_i64 = extend_to_i64(fb, n_v);
                    let elem_v = fb.ins().iconst(types::I64, layout.flex_elem_size);
                    let extra = fb.ins().imul(n_i64, elem_v);
                    let base = fb.ins().iconst(types::I64, layout.c_size.max(0));
                    fb.ins().iadd(base, extra)
                } else {
                    fb.ins().iconst(types::I64, layout.c_size.max(1))
                };
                let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
                let alloc_call = fb.ins().call(alloc_ref, &[size_v]);
                let ptr = fb.inst_results(alloc_call)[0];
                vmap.insert(*dst, ptr);
                return Ok(());
            }
            let n_fields = layout.fields.len() as i64;
            let size = fb.ins().iconst(types::I64, OBJECT_HEADER_BYTES as i64 + n_fields * 8);
            let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
            let alloc_call = fb.ins().call(alloc_ref, &[size]);
            let ptr = fb.inst_results(alloc_call)[0];
            // Store the GLOBAL class id at obj+0 — release_object,
            // host_print_object and __virt_dispatch all key off this.
            let cid_v = fb.ins().iconst(types::I64, class_global[class.0 as usize] as i64);
            fb.ins().store(MemFlags::trusted(), cid_v, ptr, 0);
            let one = fb.ins().iconst(types::I64, 1);
            fb.ins().store(MemFlags::trusted(), one, ptr, 8);

            if init.0 != u32::MAX {
                let cid = *fn_ids.get(init).ok_or_else(|| {
                    CompileError::Other(format!("missing init fn id #{}", init.0))
                })?;
                let local_ref = module.declare_func_in_func(cid, fb.func);
                let mut args: Vec<Value> = Vec::with_capacity(init_args.len() + 2);
                args.push(ptr);
                for a in init_args.iter() {
                    args.push(vmap[a]);
                }
                // Trailing env-ptr (unused by init).
                let zero = fb.ins().iconst(types::I64, 0);
                args.push(zero);
                let call_inst = fb.ins().call(local_ref, &args);
                // init returns `this`; use it (in case the runtime
                // ever wraps the receiver).
                let returned = fb.inst_results(call_inst).first().copied();
                let result = returned.unwrap_or(ptr);
                vmap.insert(*dst, result);
            } else {
                vmap.insert(*dst, ptr);
            }
        }
        Inst::NewArray { dst, elem, items } => {
            // Inline fixed-length output (when the dst MirTy carries
            // `len: Some(n)`): allocate `n*stride` bytes with no
            // header, store elements directly at `data + i*stride`.
            // This keeps the layout consistent with array fields of
            // `@extern(C)` structs that LoadField returns as inline
            // addresses.
            let dst_ty = func.ty_of(*dst).clone();
            if let MirTy::Array { len: Some(_), .. } = &dst_ty {
                let stride_bytes = elem_byte_stride(elem);
                let n = items.len() as i64;
                let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
                let bytes = fb.ins().iconst(types::I64, n.max(1) * stride_bytes);
                let call = fb.ins().call(alloc_ref, &[bytes]);
                let ptr = fb.inst_results(call)[0];
                let elem_clif_opt = elem_clif_type(elem);
                for (i, it) in items.iter().enumerate() {
                    let raw = vmap[it];
                    let off = (i as i32) * (stride_bytes as i32);
                    if let Some(elem_ct) = elem_clif_opt {
                        let truncated = ireduce_or_pass(fb, raw, elem_ct);
                        fb.ins().store(MemFlags::trusted(), truncated, ptr, off);
                    } else {
                        let v_ext = extend_to_i64(fb, raw);
                        fb.ins().store(MemFlags::trusted(), v_ext, ptr, off);
                    }
                }
                vmap.insert(*dst, ptr);
                return Ok(());
            }
            // Layout: 6-i64 header [len | cap | data_ptr | rc | kind_tag | stride]
            // + separately-allocated `stride×capacity` buffer. stride is
            // 1/2/4/8 picked from `elem` so `u8[]` / `u16[]` / `u32[]`
            // pack tightly enough for native memcpy/memset to land on
            // the right slots.
            let stride_bytes = elem_byte_stride(elem);
            let n = items.len() as i64;
            let header_bytes = fb.ins().iconst(types::I64, 48);
            let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
            let call = fb.ins().call(alloc_ref, &[header_bytes]);
            let ptr = fb.inst_results(call)[0];
            let data_bytes = fb.ins().iconst(types::I64, n.max(1) * stride_bytes);
            let dcall = fb.ins().call(alloc_ref, &[data_bytes]);
            let data_ptr = fb.inst_results(dcall)[0];

            let len_v = fb.ins().iconst(types::I64, n);
            fb.ins().store(MemFlags::trusted(), len_v, ptr, 0);
            fb.ins().store(MemFlags::trusted(), len_v, ptr, 8);
            fb.ins().store(MemFlags::trusted(), data_ptr, ptr, 16);
            let one = fb.ins().iconst(types::I64, 1);
            fb.ins().store(MemFlags::trusted(), one, ptr, 24);
            let tag = kind_tag_of(elem);
            let tag_v = fb.ins().iconst(types::I64, tag);
            fb.ins().store(MemFlags::trusted(), tag_v, ptr, 32);
            let stride_v = fb.ins().iconst(types::I64, stride_bytes);
            fb.ins().store(MemFlags::trusted(), stride_v, ptr, 40);
            let elem_clif_opt = elem_clif_type(elem);
            for (i, it) in items.iter().enumerate() {
                let raw = vmap[it];
                let off = (i as i32) * (stride_bytes as i32);
                if let Some(elem_ct) = elem_clif_opt {
                    let truncated = ireduce_or_pass(fb, raw, elem_ct);
                    fb.ins().store(MemFlags::trusted(), truncated, data_ptr, off);
                } else {
                    let v_ext = extend_to_i64(fb, raw);
                    fb.ins().store(MemFlags::trusted(), v_ext, data_ptr, off);
                }
            }
            vmap.insert(*dst, ptr);
        }
        Inst::NewArrayEmpty { dst, elem, fixed_len } => {
            let stride_bytes = elem_byte_stride(elem);
            let n = fixed_len.unwrap_or(0) as i64;
            let header_bytes = fb.ins().iconst(types::I64, 48);
            let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
            let call = fb.ins().call(alloc_ref, &[header_bytes]);
            let ptr = fb.inst_results(call)[0];
            let cap = n.max(4);
            let data_bytes = fb.ins().iconst(types::I64, cap * stride_bytes);
            let dcall = fb.ins().call(alloc_ref, &[data_bytes]);
            let data_ptr = fb.inst_results(dcall)[0];
            let len_v = fb.ins().iconst(types::I64, n);
            let cap_v = fb.ins().iconst(types::I64, cap);
            fb.ins().store(MemFlags::trusted(), len_v, ptr, 0);
            fb.ins().store(MemFlags::trusted(), cap_v, ptr, 8);
            fb.ins().store(MemFlags::trusted(), data_ptr, ptr, 16);
            let one = fb.ins().iconst(types::I64, 1);
            fb.ins().store(MemFlags::trusted(), one, ptr, 24);
            let tag = kind_tag_of(elem);
            let tag_v = fb.ins().iconst(types::I64, tag);
            fb.ins().store(MemFlags::trusted(), tag_v, ptr, 32);
            let stride_v = fb.ins().iconst(types::I64, stride_bytes);
            fb.ins().store(MemFlags::trusted(), stride_v, ptr, 40);
            vmap.insert(*dst, ptr);
        }
        Inst::ArrayLen { dst, arr } => {
            let arr_ty = func.ty_of(*arr).clone();
            let v = if let MirTy::Array { len: Some(n), .. } = &arr_ty {
                fb.ins().iconst(types::I64, *n as i64)
            } else {
                let p = vmap[arr];
                fb.ins().load(types::I64, MemFlags::trusted(), p, 0)
            };
            vmap.insert(*dst, v);
        }
        Inst::ArrayLoad { dst, arr, idx } => {
            let p = vmap[arr];
            let i_raw = vmap[idx];
            // Index may come in as a narrower int (i32 / u32 / etc.)
            // when the source code uses an int-typed counter. The
            // OOB check + offset arithmetic below all run on i64,
            // so widen up-front rather than threading a sign-cross
            // cast at every consumer.
            let i = extend_to_i64(fb, i_raw);
            // Inline fixed-size array (`u8[4]` field of an @extern(C)
            // struct, etc) — base ptr is the start of the elements,
            // no header. Use the static elem stride from the type.
            let arr_ty = func.ty_of(*arr).clone();
            let inline_info = match &arr_ty {
                MirTy::Array { elem, len: Some(n) } => {
                    Some((elem_byte_stride(elem), *n as i64))
                }
                _ => None,
            };
            let (data_ptr, stride) = if let Some((s, n)) = inline_info {
                let n_v = fb.ins().iconst(types::I64, n);
                let oob_lo = fb.ins().icmp_imm(IntCC::SignedLessThan, i, 0);
                let oob_hi = fb.ins().icmp(IntCC::SignedGreaterThanOrEqual, i, n_v);
                let oob = fb.ins().bor(oob_lo, oob_hi);
                emit_panic_if(fb, module, panic_aux.fn_id, panic_aux.msg_oob, oob);
                let s_v = fb.ins().iconst(types::I64, s);
                (p, s_v)
            } else {
                let len = fb.ins().load(types::I64, MemFlags::trusted(), p, 0);
                let oob_lo = fb.ins().icmp_imm(IntCC::SignedLessThan, i, 0);
                let oob_hi = fb.ins().icmp(IntCC::SignedGreaterThanOrEqual, i, len);
                let oob = fb.ins().bor(oob_lo, oob_hi);
                emit_panic_if(fb, module, panic_aux.fn_id, panic_aux.msg_oob, oob);
                let data_ptr = fb.ins().load(types::I64, MemFlags::trusted(), p, 16);
                let stride = fb.ins().load(types::I64, MemFlags::trusted(), p, 40);
                (data_ptr, stride)
            };
            let off = fb.ins().imul(i, stride);
            let addr = fb.ins().iadd(data_ptr, off);
            let dst_ty_mir = func.ty_of(*dst);
            let v = match elem_clif_type(dst_ty_mir) {
                Some(elem_ct) if elem_ct == types::I8 => {
                    fb.ins().load(types::I8, MemFlags::trusted(), addr, 0)
                }
                Some(elem_ct) if elem_ct == types::I16 => {
                    fb.ins().load(types::I16, MemFlags::trusted(), addr, 0)
                }
                Some(elem_ct) if elem_ct == types::I32 => {
                    fb.ins().load(types::I32, MemFlags::trusted(), addr, 0)
                }
                Some(elem_ct) if elem_ct == types::F32 => {
                    fb.ins().load(types::F32, MemFlags::trusted(), addr, 0)
                }
                Some(elem_ct) if elem_ct == types::F64 => {
                    fb.ins().load(types::F64, MemFlags::trusted(), addr, 0)
                }
                _ => {
                    let raw = fb.ins().load(types::I64, MemFlags::trusted(), addr, 0);
                    reduce_from_i64(fb, dst_ty_mir, raw)
                }
            };
            vmap.insert(*dst, v);
        }
        Inst::ArrayStore { arr, idx, value } => {
            let p = vmap[arr];
            let i_raw = vmap[idx];
            let i = extend_to_i64(fb, i_raw);
            let arr_ty = func.ty_of(*arr).clone();
            let inline_info = match &arr_ty {
                MirTy::Array { elem, len: Some(n) } => {
                    Some((elem_byte_stride(elem), *n as i64))
                }
                _ => None,
            };
            let (data_ptr, stride) = if let Some((s, n)) = inline_info {
                let n_v = fb.ins().iconst(types::I64, n);
                let oob_lo = fb.ins().icmp_imm(IntCC::SignedLessThan, i, 0);
                let oob_hi = fb.ins().icmp(IntCC::SignedGreaterThanOrEqual, i, n_v);
                let oob = fb.ins().bor(oob_lo, oob_hi);
                emit_panic_if(fb, module, panic_aux.fn_id, panic_aux.msg_oob, oob);
                let s_v = fb.ins().iconst(types::I64, s);
                (p, s_v)
            } else {
                let len = fb.ins().load(types::I64, MemFlags::trusted(), p, 0);
                let oob_lo = fb.ins().icmp_imm(IntCC::SignedLessThan, i, 0);
                let oob_hi = fb.ins().icmp(IntCC::SignedGreaterThanOrEqual, i, len);
                let oob = fb.ins().bor(oob_lo, oob_hi);
                emit_panic_if(fb, module, panic_aux.fn_id, panic_aux.msg_oob, oob);
                let data_ptr = fb.ins().load(types::I64, MemFlags::trusted(), p, 16);
                let stride = fb.ins().load(types::I64, MemFlags::trusted(), p, 40);
                (data_ptr, stride)
            };
            let off = fb.ins().imul(i, stride);
            let addr = fb.ins().iadd(data_ptr, off);
            let val_ty_mir = func.ty_of(*value);
            let raw = vmap[value];
            match elem_clif_type(val_ty_mir) {
                Some(elem_ct) if elem_ct != types::I64 => {
                    let truncated = ireduce_or_pass(fb, raw, elem_ct);
                    fb.ins().store(MemFlags::trusted(), truncated, addr, 0);
                }
                _ => {
                    let v_ext = extend_to_i64(fb, raw);
                    fb.ins().store(MemFlags::trusted(), v_ext, addr, 0);
                }
            }
        }
        Inst::NewMap { dst, key, val, entries } => {
            let new_ref = module.declare_func_in_func(map_ids.new, fb.func);
            let call = fb.ins().call(new_ref, &[]);
            let map_ptr = fb.inst_results(call)[0];
            if matches!(val, MirTy::Object(_)) {
                let mark_ref =
                    module.declare_func_in_func(panic_aux.map_set_obj_val, fb.func);
                fb.ins().call(mark_ref, &[map_ptr]);
            }
            // Tag the map with key/value print-kind ids so
            // `console.log(map)` can format entries correctly.
            let kk = fb.ins().iconst(types::I64, print_kind_id(key));
            let vk = fb.ins().iconst(types::I64, print_kind_id(val));
            let pk_ref =
                module.declare_func_in_func(panic_aux.map_set_print_kinds, fb.func);
            fb.ins().call(pk_ref, &[map_ptr, kk, vk]);
            let set_ref = module.declare_func_in_func(map_ids.set, fb.func);
            for (k, v) in entries.iter() {
                let kv = extend_to_i64(fb, vmap[k]);
                let vv = extend_to_i64(fb, vmap[v]);
                fb.ins().call(set_ref, &[map_ptr, kv, vv]);
            }
            vmap.insert(*dst, map_ptr);
        }
        Inst::MapGet { dst, map, key } => {
            let m = vmap[map];
            let k = extend_to_i64(fb, vmap[key]);
            let get_ref = module.declare_func_in_func(map_ids.get, fb.func);
            let call = fb.ins().call(get_ref, &[m, k]);
            let raw = fb.inst_results(call)[0];
            let dst_ty = func.ty_of(*dst).clone();
            let v = reduce_from_i64(fb, &dst_ty, raw);
            vmap.insert(*dst, v);
        }
        Inst::MapSet { map, key, value } => {
            let m = vmap[map];
            let k = extend_to_i64(fb, vmap[key]);
            let v = extend_to_i64(fb, vmap[value]);
            let set_ref = module.declare_func_in_func(map_ids.set, fb.func);
            fb.ins().call(set_ref, &[m, k, v]);
        }
        Inst::NewEnum { dst, enum_id, variant, payload } => {
            let layout = &prog.enums[enum_id.0 as usize];
            let v = &layout.variants[variant.0 as usize];
            let n_payload = match &v.payload {
                ilang_mir::VariantPayload::Unit => 0i64,
                ilang_mir::VariantPayload::Tuple(ts) => ts.len() as i64,
                ilang_mir::VariantPayload::Struct(fs) => fs.len() as i64,
            };
            // Unit-variant fast path: every `EnumName.unitVariant`
            // expression is value-equivalent (just a tag), so dispatch
            // through a process-wide cache keyed by
            // (global_enum_id, discriminant). Avoids the 8-byte
            // alloc-per-call leak for things like
            // `gamepad.isPressed(sdl.Button.a)` in a 60fps loop —
            // those fired ~840×/sec before this change.
            if n_payload == 0 {
                let global = enum_global[enum_id.0 as usize] as i64;
                let global_v = fb.ins().iconst(types::I64, global);
                let disc_v = fb.ins().iconst(types::I64, v.discriminant);
                let f = module.declare_func_in_func(panic_aux.enum_unit_get, fb.func);
                let call = fb.ins().call(f, &[global_v, disc_v]);
                let ptr = fb.inst_results(call)[0];
                vmap.insert(*dst, ptr);
                return Ok(());
            }
            // Payload variant — register with the rc-tracked enum
            // registry via __enum_alloc so the cell can be freed on
            // rc=0. Layout still `[tag | payload...]`; the registry
            // sits beside the cell holding (rc, total_bytes).
            let global = enum_global[enum_id.0 as usize] as i64;
            let global_v = fb.ins().iconst(types::I64, global);
            let n_v = fb.ins().iconst(types::I64, n_payload);
            let disc_v = fb.ins().iconst(types::I64, v.discriminant);
            let alloc_fn = module.declare_func_in_func(panic_aux.enum_alloc, fb.func);
            let call = fb.ins().call(alloc_fn, &[global_v, n_v, disc_v]);
            let ptr = fb.inst_results(call)[0];
            for (i, p) in payload.iter().enumerate() {
                let v_ext = extend_to_i64(fb, vmap[p]);
                fb.ins().store(
                    MemFlags::trusted(),
                    v_ext,
                    ptr,
                    8 + (i as i32) * 8,
                );
            }
            vmap.insert(*dst, ptr);
        }
        Inst::EnumTag { dst, value } => {
            let p = vmap[value];
            let v = fb.ins().load(types::I64, MemFlags::trusted(), p, 0);
            vmap.insert(*dst, v);
        }
        Inst::EnumPayload { dst, value, variant: _, idx } => {
            let p = vmap[value];
            let off = 8 + (*idx as i32) * 8;
            let raw = fb.ins().load(types::I64, MemFlags::trusted(), p, off);
            let dst_ty = func.ty_of(*dst).clone();
            let v = reduce_from_i64(fb, &dst_ty, raw);
            // Heap-typed payload extraction transfers ownership: the
            // extract sees the cell's stored +1 and gives the caller
            // its own +1. Pairs with `host_release_enum`'s cascade
            // on the cell's drop — without the retain, the
            // arm-scope release of the extracted binding would
            // double-decrement and either dangle (cell still holds
            // the ptr) or crash on subsequent access.
            let kind = kind_tag_of(&dst_ty);
            if kind != KIND_NONE {
                let r = match kind {
                    KIND_OBJECT => panic_aux.retain_obj,
                    KIND_ARRAY => panic_aux.retain_array,
                    KIND_OPTIONAL => panic_aux.retain_optional,
                    KIND_TUPLE => panic_aux.retain_tuple,
                    KIND_MAP => panic_aux.retain_map,
                    KIND_CLOSURE => panic_aux.retain_closure,
                    KIND_STR => panic_aux.retain_string,
                    KIND_ENUM => panic_aux.retain_enum,
                    _ => unreachable!(),
                };
                let f = module.declare_func_in_func(r, fb.func);
                fb.ins().call(f, &[v]);
            }
            vmap.insert(*dst, v);
        }
        Inst::NewTuple { dst, items } => {
            // Heterogeneous fixed-arity product. Hidden 16-byte
            // header lives BEFORE the user-facing pointer:
            //   base + 0  = rc
            //   base + 8  = packed:
            //                 bits  0-15 = arity (max 65535)
            //                 bits 16-63 = 4-bit KIND_* tag per
            //                              element (up to 12 elements;
            //                              elements 12+ leak any
            //                              heap content but the cell
            //                              itself is still freed).
            //   base + 16 = element 0 ← user_ptr
            // TupleExtract reads from offset 0 of user_ptr, unchanged.
            let n = items.len() as i64;
            let bytes = fb.ins().iconst(types::I64, 16 + n.max(1) * 8);
            let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
            let call = fb.ins().call(alloc_ref, &[bytes]);
            let base = fb.inst_results(call)[0];
            let off16 = fb.ins().iconst(types::I64, 16);
            let ptr = fb.ins().iadd(base, off16);
            // rc = 1
            let one = fb.ins().iconst(types::I64, 1);
            fb.ins().store(MemFlags::trusted(), one, base, 0);
            // packed (kinds | arity)
            let dst_ty = func.ty_of(*dst).clone();
            let mut packed: i64 = n & 0xFFFF;
            if let MirTy::Tuple(elems) = &dst_ty {
                for (i, ety) in elems.iter().enumerate() {
                    if i >= 12 {
                        break;
                    }
                    let kind = kind_tag_of(ety) & 0xF;
                    packed |= kind << (16 + (i as i64) * 4);
                }
            }
            let mask_v = fb.ins().iconst(types::I64, packed);
            fb.ins().store(MemFlags::trusted(), mask_v, base, 8);
            for (i, it) in items.iter().enumerate() {
                let v_ext = extend_to_i64(fb, vmap[it]);
                fb.ins().store(MemFlags::trusted(), v_ext, ptr, (i as i32) * 8);
            }
            vmap.insert(*dst, ptr);
        }
        Inst::TupleExtract { dst, tup, idx } => {
            let p = vmap[tup];
            let off = (*idx as i32) * 8;
            let raw = fb.ins().load(types::I64, MemFlags::trusted(), p, off);
            let dst_ty = func.ty_of(*dst).clone();
            let v = reduce_from_i64(fb, &dst_ty, raw);
            vmap.insert(*dst, v);
        }
        Inst::NewOptional { dst, value } => {
            // `some(v)` → allocate a 3-cell heap [value | rc | kind_tag]
            // and return its address. `value` is at offset 0 so existing
            // unwrap / iflet paths keep reading from offset 0.
            let bytes = fb.ins().iconst(types::I64, 24);
            let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
            let call = fb.ins().call(alloc_ref, &[bytes]);
            let ptr = fb.inst_results(call)[0];
            let v_ext = extend_to_i64(fb, vmap[value]);
            fb.ins().store(MemFlags::trusted(), v_ext, ptr, 0);
            let one = fb.ins().iconst(types::I64, 1);
            fb.ins().store(MemFlags::trusted(), one, ptr, 8);
            // Tag from the dst's static type — kind_tag mirrors the
            // Array convention: KIND_* discriminant of the inner
            // type so host_release_optional can dispatch the right
            // release fn at cascade time.
            let dst_ty = func.ty_of(*dst).clone();
            let tag = if let MirTy::Optional(inner) = &dst_ty {
                kind_tag_of(inner)
            } else {
                KIND_NONE
            };
            let tag_v = fb.ins().iconst(types::I64, tag);
            fb.ins().store(MemFlags::trusted(), tag_v, ptr, 16);
            vmap.insert(*dst, ptr);
        }
        Inst::OptionalIsSome { dst, opt } => {
            let p = vmap[opt];
            let zero = fb.ins().iconst(types::I64, 0);
            let v = fb.ins().icmp(IntCC::NotEqual, p, zero);
            vmap.insert(*dst, v);
        }
        Inst::OptionalUnwrap { dst, opt } => {
            let p = vmap[opt];
            let zero = fb.ins().iconst(types::I64, 0);
            let is_none = fb.ins().icmp(IntCC::Equal, p, zero);
            emit_panic_if(fb, module, panic_aux.fn_id, panic_aux.msg_unwrap, is_none);
            let raw = fb.ins().load(types::I64, MemFlags::trusted(), p, 0);
            let dst_ty = func.ty_of(*dst).clone();
            let v = reduce_from_i64(fb, &dst_ty, raw);
            vmap.insert(*dst, v);
        }
        Inst::LoadField { dst, obj, field } => {
            let obj_v = vmap[obj];
            let dst_ty_mir = func.ty_of(*dst).clone();
            let obj_ty_mir = func.ty_of(*obj).clone();
            let (crepr, bit_info) = if let MirTy::Object(cid) = &obj_ty_mir {
                let layout = &prog.classes[cid.0 as usize];
                if matches!(
                    layout.repr,
                    ilang_mir::ClassRepr::CRepr
                        | ilang_mir::ClassRepr::CPacked
                        | ilang_mir::ClassRepr::CUnion
                ) {
                    let off = layout.c_field_offsets.get(field.0 as usize).copied().unwrap_or(0);
                    let bf = layout
                        .fields
                        .get(field.0 as usize)
                        .and_then(|f| f.bit_field);
                    (Some(off), bf)
                } else {
                    (None, None)
                }
            } else {
                (None, None)
            };
            // Bitfield read: load the storage unit, shift right by
            // bit_offset, mask off the high bits beyond `width`.
            if let (Some(c_off), Some(bf)) = (crepr, bit_info) {
                let storage_ct = match elem_clif_type(&dst_ty_mir) {
                    Some(t) if t.bits() <= 32 => t,
                    _ => types::I32,
                };
                let raw = fb.ins().load(
                    storage_ct,
                    MemFlags::trusted(),
                    obj_v,
                    c_off as i32,
                );
                let shifted = if bf.offset == 0 {
                    raw
                } else {
                    let shift = fb.ins().iconst(storage_ct, bf.offset as i64);
                    fb.ins().ushr(raw, shift)
                };
                let mask_val: u64 = if bf.width >= 64 {
                    u64::MAX
                } else {
                    (1u64 << bf.width) - 1
                };
                let mask = fb.ins().iconst(storage_ct, mask_val as i64);
                let v = fb.ins().band(shifted, mask);
                vmap.insert(*dst, v);
                return Ok(());
            }
            // FAM (C99 flexible array member) — last field of a CRepr
            // struct typed `T[]` (no len). The field has no slot of
            // its own; its data starts at obj_v + c_off and runs to
            // the end of the over-allocated buffer. We don't know the
            // element count statically (caller maintains it in a
            // sibling field), so wrap the inline area in a synthetic
            // dyn-array header with len=i64::MAX so subsequent
            // ArrayLoad / ArrayStore bounds checks become no-ops, but
            // the data pointer aliases the inline buffer so reads
            // and writes hit the real storage.
            if let Some(c_off) = crepr {
                let is_fam = matches!(&dst_ty_mir, MirTy::Array { len: None, .. })
                    && matches!(
                        &obj_ty_mir,
                        MirTy::Object(_cid)
                    );
                if is_fam {
                    if let MirTy::Object(cid) = &obj_ty_mir {
                        let layout = &prog.classes[cid.0 as usize];
                        let last_ix = layout.fields.len().saturating_sub(1);
                        if field.0 as usize == last_ix && layout.flex_elem_size > 0 {
                            let elem = if let MirTy::Array { elem, .. } = &dst_ty_mir {
                                (**elem).clone()
                            } else {
                                MirTy::I64
                            };
                            let stride = layout.flex_elem_size;
                            let kind_tag = if matches!(elem, MirTy::Object(_)) {
                                1
                            } else {
                                0
                            };
                            let off_v = fb.ins().iconst(types::I64, c_off);
                            let inline_ptr = fb.ins().iadd(obj_v, off_v);
                            let len_v = fb.ins().iconst(types::I64, i64::MAX);
                            let stride_v = fb.ins().iconst(types::I64, stride);
                            let kind_v = fb.ins().iconst(types::I64, kind_tag);
                            let f = module.declare_func_in_func(str_ids.fixed_to_dyn, fb.func);
                            let call = fb.ins().call(f, &[inline_ptr, len_v, stride_v, kind_v]);
                            let v = fb.inst_results(call)[0];
                            vmap.insert(*dst, v);
                            return Ok(());
                        }
                    }
                }
                // CRepr: load with the field's natural type at the
                // computed byte offset. Nested CRepr struct fields
                // return the inline address.
                let v = match elem_clif_type(&dst_ty_mir) {
                    Some(elem_ct) if elem_ct == types::I8 => {
                        fb.ins().load(types::I8, MemFlags::trusted(), obj_v, c_off as i32)
                    }
                    Some(elem_ct) if elem_ct == types::I16 => {
                        fb.ins().load(types::I16, MemFlags::trusted(), obj_v, c_off as i32)
                    }
                    Some(elem_ct) if elem_ct == types::I32 => {
                        fb.ins().load(types::I32, MemFlags::trusted(), obj_v, c_off as i32)
                    }
                    Some(elem_ct) if elem_ct == types::F32 => {
                        fb.ins().load(types::F32, MemFlags::trusted(), obj_v, c_off as i32)
                    }
                    Some(elem_ct) if elem_ct == types::F64 => {
                        fb.ins().load(types::F64, MemFlags::trusted(), obj_v, c_off as i32)
                    }
                    _ => {
                        // Nested CRepr struct, fixed-size array, or
                        // i64-sized field — produce the inline address
                        // (additive offset) for composites, otherwise
                        // load the i64 cell.
                        let returns_inline = match &dst_ty_mir {
                            MirTy::Object(inner_cid) => matches!(
                                prog.classes[inner_cid.0 as usize].repr,
                                ilang_mir::ClassRepr::CRepr
                                    | ilang_mir::ClassRepr::CPacked
                                    | ilang_mir::ClassRepr::CUnion
                            ),
                            MirTy::Array { len: Some(_), .. } => true,
                            _ => false,
                        };
                        if returns_inline {
                            let off_v = fb.ins().iconst(types::I64, c_off);
                            fb.ins().iadd(obj_v, off_v)
                        } else {
                            fb.ins().load(
                                types::I64,
                                MemFlags::trusted(),
                                obj_v,
                                c_off as i32,
                            )
                        }
                    }
                };
                vmap.insert(*dst, v);
            } else {
                let off = OBJECT_HEADER_BYTES + (field.0 as i32) * 8;
                let raw = fb.ins().load(types::I64, MemFlags::trusted(), obj_v, off);
                let v = reduce_from_i64(fb, &dst_ty_mir, raw);
                vmap.insert(*dst, v);
            }
        }
        Inst::StoreField { obj, field, value } => {
            let obj_v = vmap[obj];
            let obj_ty_mir = func.ty_of(*obj).clone();
            let (crepr, bit_info) = if let MirTy::Object(cid) = &obj_ty_mir {
                let layout = &prog.classes[cid.0 as usize];
                if matches!(
                    layout.repr,
                    ilang_mir::ClassRepr::CRepr
                        | ilang_mir::ClassRepr::CPacked
                        | ilang_mir::ClassRepr::CUnion
                ) {
                    let off = layout.c_field_offsets.get(field.0 as usize).copied().unwrap_or(0);
                    let bf = layout
                        .fields
                        .get(field.0 as usize)
                        .and_then(|f| f.bit_field);
                    (Some(off), bf)
                } else {
                    (None, None)
                }
            } else {
                (None, None)
            };
            // Bitfield write: read-modify-write: load storage, mask
            // off the field's bits, OR in the new value's bits at
            // the right offset, store back.
            if let (Some(c_off), Some(bf)) = (crepr, bit_info) {
                let val_ty_mir = func.ty_of(*value).clone();
                let raw_val = vmap[value];
                let storage_ct = match elem_clif_type(&val_ty_mir) {
                    Some(t) if t.bits() <= 32 => t,
                    _ => types::I32,
                };
                let cur = fb.ins().load(
                    storage_ct,
                    MemFlags::trusted(),
                    obj_v,
                    c_off as i32,
                );
                let mask_val: u64 = if bf.width >= 64 {
                    u64::MAX
                } else {
                    (1u64 << bf.width) - 1
                };
                let inv_mask_val = !(mask_val << bf.offset);
                let inv_mask = fb.ins().iconst(storage_ct, inv_mask_val as i64);
                let cleared = fb.ins().band(cur, inv_mask);
                let v_truncated = ireduce_or_pass(fb, raw_val, storage_ct);
                let mask = fb.ins().iconst(storage_ct, mask_val as i64);
                let v_masked = fb.ins().band(v_truncated, mask);
                let v_shifted = if bf.offset == 0 {
                    v_masked
                } else {
                    let shift = fb.ins().iconst(storage_ct, bf.offset as i64);
                    fb.ins().ishl(v_masked, shift)
                };
                let new_val = fb.ins().bor(cleared, v_shifted);
                fb.ins().store(MemFlags::trusted(), new_val, obj_v, c_off as i32);
                return Ok(());
            }
            if let Some(c_off) = crepr {
                let val_ty_mir = func.ty_of(*value).clone();
                let raw = vmap[value];
                // If the field type is itself a CRepr struct, copy
                // the source struct's bytes into the destination's
                // inline region rather than storing the pointer.
                if let MirTy::Object(inner_cid) = &val_ty_mir {
                    let inner_layout = &prog.classes[inner_cid.0 as usize];
                    if matches!(
                        inner_layout.repr,
                        ilang_mir::ClassRepr::CRepr
                            | ilang_mir::ClassRepr::CPacked
                            | ilang_mir::ClassRepr::CUnion
                    ) {
                        let dst_addr = if c_off == 0 {
                            obj_v
                        } else {
                            let off_v = fb.ins().iconst(types::I64, c_off);
                            fb.ins().iadd(obj_v, off_v)
                        };
                        // Inline byte-wise copy of `c_size` bytes —
                        // avoids depending on the JIT's memcpy libcall
                        // resolution, which can race with how mir-codegen
                        // declares its own symbols.
                        let total = inner_layout.c_size.max(0);
                        let mut copied = 0i64;
                        while copied + 8 <= total {
                            let v = fb.ins().load(
                                types::I64,
                                MemFlags::trusted(),
                                raw,
                                copied as i32,
                            );
                            fb.ins().store(
                                MemFlags::trusted(),
                                v,
                                dst_addr,
                                copied as i32,
                            );
                            copied += 8;
                        }
                        while copied + 4 <= total {
                            let v = fb.ins().load(
                                types::I32,
                                MemFlags::trusted(),
                                raw,
                                copied as i32,
                            );
                            fb.ins().store(
                                MemFlags::trusted(),
                                v,
                                dst_addr,
                                copied as i32,
                            );
                            copied += 4;
                        }
                        while copied + 2 <= total {
                            let v = fb.ins().load(
                                types::I16,
                                MemFlags::trusted(),
                                raw,
                                copied as i32,
                            );
                            fb.ins().store(
                                MemFlags::trusted(),
                                v,
                                dst_addr,
                                copied as i32,
                            );
                            copied += 2;
                        }
                        while copied < total {
                            let v = fb.ins().load(
                                types::I8,
                                MemFlags::trusted(),
                                raw,
                                copied as i32,
                            );
                            fb.ins().store(
                                MemFlags::trusted(),
                                v,
                                dst_addr,
                                copied as i32,
                            );
                            copied += 1;
                        }
                        return Ok(());
                    }
                }
                match elem_clif_type(&val_ty_mir) {
                    Some(elem_ct) if elem_ct != types::I64 => {
                        let truncated = ireduce_or_pass(fb, raw, elem_ct);
                        fb.ins().store(MemFlags::trusted(), truncated, obj_v, c_off as i32);
                    }
                    _ => {
                        let v_ext = extend_to_i64(fb, raw);
                        fb.ins().store(MemFlags::trusted(), v_ext, obj_v, c_off as i32);
                    }
                }
            } else {
                let off = OBJECT_HEADER_BYTES + (field.0 as i32) * 8;
                let store_v = extend_to_i64(fb, vmap[value]);
                fb.ins().store(MemFlags::trusted(), store_v, obj_v, off);
            }
        }
        Inst::LoadStatic { dst, slot } => {
            let did = *static_data.get(slot).ok_or_else(|| {
                CompileError::Other(format!("missing static data slot #{}", slot.0))
            })?;
            let gv = module.declare_data_in_func(did, fb.func);
            let addr = fb
                .ins()
                .symbol_value(types::I64, gv);
            // Load type matches the slot's declared MirTy.
            let s = &prog.statics[slot.0 as usize];
            let raw = fb.ins().load(types::I64, MemFlags::trusted(), addr, 0);
            let v = match &s.ty {
                MirTy::I64 | MirTy::U64 | MirTy::Size | MirTy::SSize => raw,
                MirTy::I32 | MirTy::U32 => fb.ins().ireduce(types::I32, raw),
                MirTy::I16 | MirTy::U16 => fb.ins().ireduce(types::I16, raw),
                MirTy::I8 | MirTy::U8 | MirTy::Bool => fb.ins().ireduce(types::I8, raw),
                MirTy::F64 => fb.ins().bitcast(types::F64, MemFlags::new(), raw),
                MirTy::F32 => {
                    let r32 = fb.ins().ireduce(types::I32, raw);
                    fb.ins().bitcast(types::F32, MemFlags::new(), r32)
                }
                _ => return Err(CompileError::Unsupported("static slot type")),
            };
            vmap.insert(*dst, v);
        }
        Inst::StoreStatic { slot, value } => {
            let did = *static_data.get(slot).ok_or_else(|| {
                CompileError::Other(format!("missing static data slot #{}", slot.0))
            })?;
            let gv = module.declare_data_in_func(did, fb.func);
            let addr = fb.ins().symbol_value(types::I64, gv);
            let v = vmap[value];
            let s = &prog.statics[slot.0 as usize];
            let store_v = match &s.ty {
                MirTy::I64 | MirTy::U64 | MirTy::Size | MirTy::SSize => v,
                MirTy::I32 | MirTy::U32 | MirTy::I16 | MirTy::U16 | MirTy::I8 | MirTy::U8
                | MirTy::Bool => fb.ins().uextend(types::I64, v),
                MirTy::F64 => fb.ins().bitcast(types::I64, MemFlags::new(), v),
                MirTy::F32 => {
                    let r32 = fb.ins().bitcast(types::I32, MemFlags::new(), v);
                    fb.ins().uextend(types::I64, r32)
                }
                _ => return Err(CompileError::Unsupported("static slot store type")),
            };
            fb.ins().store(MemFlags::trusted(), store_v, addr, 0);
        }
        _ => {
            return Err(CompileError::Unsupported(
                "MIR inst kind not yet wired in mir-codegen",
            ));
        }
    }
    Ok(())
}

fn lower_term(
    fb: &mut ClifFnBuilder,
    term: &Terminator,
    vmap: &HashMap<ValueId, Value>,
    blocks: &[cranelift::prelude::Block],
) -> Result<(), CompileError> {
    // Helper: keep only values that have a clif counterpart in vmap.
    let visible = |args: &[ValueId]| -> Vec<cranelift_codegen::ir::BlockArg> {
        args.iter()
            .filter_map(|a| vmap.get(a).copied().map(|v| v.into()))
            .collect()
    };
    match term {
        Terminator::Return { value } => {
            match value.and_then(|v| vmap.get(&v).copied()) {
                Some(cv) => {
                    fb.ins().return_(&[cv]);
                }
                None => {
                    fb.ins().return_(&[]);
                }
            }
        }
        Terminator::Br { dst, args } => {
            let cb = blocks[dst.0 as usize];
            let avs = visible(args);
            fb.ins().jump(cb, avs.iter());
        }
        Terminator::CondBr {
            cond, then_block, then_args, else_block, else_args,
        } => {
            let c = vmap[cond];
            let tb = blocks[then_block.0 as usize];
            let eb = blocks[else_block.0 as usize];
            let ta = visible(then_args);
            let ea = visible(else_args);
            fb.ins().brif(c, tb, ta.iter(), eb, ea.iter());
        }
        Terminator::Switch { scrutinee, cases, default, default_args } => {
            let s = vmap[scrutinee];
            let stype = fb.func.dfg.value_type(s);
            for c in cases.iter() {
                let lit = fb.ins().iconst(stype, c.value);
                let cmp = fb.ins().icmp(IntCC::Equal, s, lit);
                let target = blocks[c.dst.0 as usize];
                let next = fb.create_block();
                let target_args = visible(&c.args);
                fb.ins().brif(cmp, target, target_args.iter(), next, &[]);
                fb.switch_to_block(next);
                fb.seal_block(next);
            }
            let dst_blk = blocks[default.0 as usize];
            let dargs = visible(default_args);
            fb.ins().jump(dst_blk, dargs.iter());
        }
        Terminator::Unreachable => {
            fb.ins().trap(TrapCode::user(1).unwrap());
        }
    }
    Ok(())
}

fn lower_const(
    fb: &mut ClifFnBuilder,
    c: &MirConst,
    ty: &MirTy,
) -> Result<Value, CompileError> {
    let ct = mir_to_clif(ty).ok_or(CompileError::Unsupported("unit const"))?;
    Ok(match c {
        MirConst::Bool(b) => fb.ins().iconst(ct, if *b { 1 } else { 0 }),
        MirConst::Int(n) => fb.ins().iconst(ct, *n),
        MirConst::F32(bits) => fb.ins().f32const(f32::from_bits(*bits)),
        MirConst::F64(bits) => fb.ins().f64const(f64::from_bits(*bits)),
        MirConst::Unit => return Err(CompileError::Unsupported("unit const")),
        MirConst::None => fb.ins().iconst(types::I64, 0),
        MirConst::Str(_) => return Err(CompileError::Unsupported("string const")),
    })
}

/// Print a literal C-string (DataId) via `__print_str`.
fn emit_print_lit(
    fb: &mut ClifFnBuilder,
    module: &mut JITModule,
    print_str: cranelift_module::FuncId,
    msg_data: DataId,
) {
    let gv = module.declare_data_in_func(msg_data, fb.func);
    let base = fb.ins().symbol_value(types::I64, gv);
    let off = fb.ins().iconst(types::I64, 8);
    let addr = fb.ins().iadd(base, off);
    let fr = module.declare_func_in_func(print_str, fb.func);
    fb.ins().call(fr, &[addr]);
}

/// Emit code that prints `value` of static type `ty`. Recurses into
/// composite types (Optional, Tuple, Array). For Map/Object/Closure/
/// Weak/Enum we fall back to printing the raw pointer (limited).
fn emit_print_value(
    fb: &mut ClifFnBuilder,
    module: &mut JITModule,
    print_ids: PrintIds,
    print_lits: PrintLits,
    ty: &MirTy,
    av: Value,
    enum_global: &[u32],
) {
    match ty {
        MirTy::Bool => {
            let v = if fb.func.dfg.value_type(av) == types::I64 {
                av
            } else {
                fb.ins().uextend(types::I64, av)
            };
            let r = module.declare_func_in_func(print_ids.bool_, fb.func);
            fb.ins().call(r, &[v]);
        }
        t if t.is_int() => {
            let v = if fb.func.dfg.value_type(av) == types::I64 {
                av
            } else if t.is_signed_int() {
                fb.ins().sextend(types::I64, av)
            } else {
                fb.ins().uextend(types::I64, av)
            };
            let r = module.declare_func_in_func(print_ids.int, fb.func);
            fb.ins().call(r, &[v]);
        }
        MirTy::F32 => {
            let v = fb.ins().fpromote(types::F64, av);
            let r = module.declare_func_in_func(print_ids.f64_, fb.func);
            fb.ins().call(r, &[v]);
        }
        MirTy::F64 => {
            let r = module.declare_func_in_func(print_ids.f64_, fb.func);
            fb.ins().call(r, &[av]);
        }
        MirTy::Str => {
            let r = module.declare_func_in_func(print_ids.str_, fb.func);
            fb.ins().call(r, &[av]);
        }
        MirTy::Optional(inner) => {
            // if av == 0 { print "none" } else { print "some(<inner>)" }
            let zero = fb.ins().iconst(types::I64, 0);
            let is_none = fb.ins().icmp(IntCC::Equal, av, zero);
            let none_blk = fb.create_block();
            let some_blk = fb.create_block();
            let cont_blk = fb.create_block();
            fb.ins().brif(is_none, none_blk, &[], some_blk, &[]);

            fb.switch_to_block(none_blk);
            fb.seal_block(none_blk);
            emit_print_lit(fb, module, print_ids.str_, print_lits.none);
            fb.ins().jump(cont_blk, [].iter());

            fb.switch_to_block(some_blk);
            fb.seal_block(some_blk);
            emit_print_lit(fb, module, print_ids.str_, print_lits.some_open);
            // Load the boxed inner value (the some payload is a 1-cell heap).
            let raw = fb.ins().load(types::I64, MemFlags::trusted(), av, 0);
            let inner_v = reduce_from_i64(fb, inner, raw);
            emit_print_value(fb, module, print_ids, print_lits, inner, inner_v, enum_global);
            emit_print_lit(fb, module, print_ids.str_, print_lits.close_paren);
            fb.ins().jump(cont_blk, [].iter());

            fb.switch_to_block(cont_blk);
            fb.seal_block(cont_blk);
        }
        MirTy::Tuple(items) => {
            emit_print_lit(fb, module, print_ids.str_, print_lits.open_paren);
            for (i, ity) in items.iter().enumerate() {
                if i > 0 {
                    emit_print_lit(fb, module, print_ids.str_, print_lits.comma_sp);
                }
                let off = (i as i32) * 8;
                let raw = fb.ins().load(types::I64, MemFlags::trusted(), av, off);
                let elem_v = reduce_from_i64(fb, ity, raw);
                emit_print_value(fb, module, print_ids, print_lits, ity, elem_v, enum_global);
            }
            emit_print_lit(fb, module, print_ids.str_, print_lits.close_paren);
        }
        MirTy::Array { elem, .. } => {
            // [len|cap|data_ptr] header.
            let len = fb.ins().load(types::I64, MemFlags::trusted(), av, 0);
            let data_ptr = fb.ins().load(types::I64, MemFlags::trusted(), av, 16);
            emit_print_lit(fb, module, print_ids.str_, print_lits.open_bracket);
            // for i in 0..len: print elem; if i+1 < len: print ", "
            let header = fb.create_block();
            fb.append_block_param(header, types::I64);
            let body_blk = fb.create_block();
            let exit_blk = fb.create_block();

            let zero = fb.ins().iconst(types::I64, 0);
            fb.ins().jump(header, [zero.into()].iter());

            fb.switch_to_block(header);
            let i_arg = fb.block_params(header)[0];
            let cond = fb.ins().icmp(IntCC::SignedLessThan, i_arg, len);
            fb.ins().brif(cond, body_blk, &[], exit_blk, &[]);

            fb.switch_to_block(body_blk);
            fb.seal_block(body_blk);
            // Print separator if i > 0.
            let zero2 = fb.ins().iconst(types::I64, 0);
            let is_first = fb.ins().icmp(IntCC::Equal, i_arg, zero2);
            let print_sep = fb.create_block();
            let after_sep = fb.create_block();
            fb.ins().brif(is_first, after_sep, &[], print_sep, &[]);

            fb.switch_to_block(print_sep);
            fb.seal_block(print_sep);
            emit_print_lit(fb, module, print_ids.str_, print_lits.comma_sp);
            fb.ins().jump(after_sep, [].iter());

            fb.switch_to_block(after_sep);
            fb.seal_block(after_sep);
            // Load elem at i.
            let stride = fb.ins().iconst(types::I64, 8);
            let off = fb.ins().imul(i_arg, stride);
            let addr = fb.ins().iadd(data_ptr, off);
            let raw = fb.ins().load(types::I64, MemFlags::trusted(), addr, 0);
            let elem_v = reduce_from_i64(fb, elem, raw);
            emit_print_value(fb, module, print_ids, print_lits, elem, elem_v, enum_global);
            // i = i + 1
            let one = fb.ins().iconst(types::I64, 1);
            let i_next = fb.ins().iadd(i_arg, one);
            fb.ins().jump(header, [i_next.into()].iter());

            fb.seal_block(header);
            fb.switch_to_block(exit_blk);
            fb.seal_block(exit_blk);
            emit_print_lit(fb, module, print_ids.str_, print_lits.close_bracket);
        }
        MirTy::Object(_) => {
            let r = module.declare_func_in_func(print_ids.object, fb.func);
            fb.ins().call(r, &[av]);
        }
        MirTy::Fn(_) => {
            let r = module.declare_func_in_func(print_ids.fn_, fb.func);
            fb.ins().call(r, &[av]);
        }
        MirTy::Map { .. } => {
            let r = module.declare_func_in_func(print_ids.map, fb.func);
            fb.ins().call(r, &[av]);
        }
        MirTy::Weak(_) => {
            let r = module.declare_func_in_func(print_ids.weak, fb.func);
            fb.ins().call(r, &[av]);
        }
        MirTy::Enum(eid) => {
            let global = enum_global[eid.0 as usize] as i64;
            let id_v = fb.ins().iconst(types::I64, global);
            let r = module.declare_func_in_func(print_ids.enum_, fb.func);
            fb.ins().call(r, &[id_v, av]);
        }
        _ => {
            // Fallback: print as raw int (pointer / tag).
            let v = extend_to_i64(fb, av);
            let r = module.declare_func_in_func(print_ids.int, fb.func);
            fb.ins().call(r, &[v]);
        }
    }
}

/// Emit `if cond_truthy { call __ilang_panic(msg); trap } else { fallthrough }`.
/// `cond` must be an i8 boolean (1 = panic). Used for div/0, OOB, unwrap-None.
fn emit_panic_if(
    fb: &mut ClifFnBuilder,
    module: &mut JITModule,
    panic_fn: cranelift_module::FuncId,
    msg_data: DataId,
    cond: Value,
) {
    let panic_block = fb.create_block();
    let cont_block = fb.create_block();
    fb.ins().brif(cond, panic_block, &[], cont_block, &[]);
    fb.switch_to_block(panic_block);
    fb.seal_block(panic_block);
    let gv = module.declare_data_in_func(msg_data, fb.func);
    let base = fb.ins().symbol_value(types::I64, gv);
    let off = fb.ins().iconst(types::I64, 8);
    let addr = fb.ins().iadd(base, off);
    let fr = module.declare_func_in_func(panic_fn, fb.func);
    fb.ins().call(fr, &[addr]);
    fb.ins().trap(TrapCode::user(1).unwrap());
    fb.switch_to_block(cont_block);
    fb.seal_block(cont_block);
}

fn lower_binop(fb: &mut ClifFnBuilder, op: BinOp, lhs: Value, rhs: Value) -> Value {
    // Defensive type-bridging: the MIR's `unify_numeric` aligns
    // operand MirTys but the AST→MIR path can leak a literal that
    // ended up wider than the binop's intended cell width (e.g. a
    // bare `1` inside `cellH - 1` where cellH is i32). Cranelift
    // requires both arithmetic / compare operands to share the
    // exact clif type, so widen/narrow the smaller operand on the
    // fly. For shifts we leave the count as-is (Cranelift accepts
    // any integer type for the shift amount).
    let (lhs, rhs) = match op {
        BinOp::IAdd
        | BinOp::ISub
        | BinOp::IMul
        | BinOp::IDivS
        | BinOp::IDivU
        | BinOp::IRemS
        | BinOp::IRemU
        | BinOp::IAnd
        | BinOp::IOr
        | BinOp::IXor
        | BinOp::IEq
        | BinOp::INe
        | BinOp::ILtS | BinOp::ILeS | BinOp::IGtS | BinOp::IGeS
        | BinOp::ILtU | BinOp::ILeU | BinOp::IGtU | BinOp::IGeU => {
            let lt = fb.func.dfg.value_type(lhs);
            let rt = fb.func.dfg.value_type(rhs);
            if lt != rt && lt.is_int() && rt.is_int() {
                if lt.bits() < rt.bits() {
                    (fb.ins().sextend(rt, lhs), rhs)
                } else {
                    (lhs, fb.ins().sextend(lt, rhs))
                }
            } else {
                (lhs, rhs)
            }
        }
        _ => (lhs, rhs),
    };
    match op {
        BinOp::IAdd => fb.ins().iadd(lhs, rhs),
        BinOp::ISub => fb.ins().isub(lhs, rhs),
        BinOp::IMul => fb.ins().imul(lhs, rhs),
        BinOp::IDivS => fb.ins().sdiv(lhs, rhs),
        BinOp::IDivU => fb.ins().udiv(lhs, rhs),
        BinOp::IRemS => fb.ins().srem(lhs, rhs),
        BinOp::IRemU => fb.ins().urem(lhs, rhs),
        BinOp::IShl => fb.ins().ishl(lhs, rhs),
        BinOp::IShrS => fb.ins().sshr(lhs, rhs),
        BinOp::IShrU => fb.ins().ushr(lhs, rhs),
        BinOp::IAnd => fb.ins().band(lhs, rhs),
        BinOp::IOr => fb.ins().bor(lhs, rhs),
        BinOp::IXor => fb.ins().bxor(lhs, rhs),
        BinOp::FAdd => fb.ins().fadd(lhs, rhs),
        BinOp::FSub => fb.ins().fsub(lhs, rhs),
        BinOp::FMul => fb.ins().fmul(lhs, rhs),
        BinOp::FDiv => fb.ins().fdiv(lhs, rhs),
        BinOp::IEq => fb.ins().icmp(IntCC::Equal, lhs, rhs),
        BinOp::INe => fb.ins().icmp(IntCC::NotEqual, lhs, rhs),
        BinOp::ILtS => fb.ins().icmp(IntCC::SignedLessThan, lhs, rhs),
        BinOp::ILeS => fb.ins().icmp(IntCC::SignedLessThanOrEqual, lhs, rhs),
        BinOp::IGtS => fb.ins().icmp(IntCC::SignedGreaterThan, lhs, rhs),
        BinOp::IGeS => fb.ins().icmp(IntCC::SignedGreaterThanOrEqual, lhs, rhs),
        BinOp::ILtU => fb.ins().icmp(IntCC::UnsignedLessThan, lhs, rhs),
        BinOp::ILeU => fb.ins().icmp(IntCC::UnsignedLessThanOrEqual, lhs, rhs),
        BinOp::IGtU => fb.ins().icmp(IntCC::UnsignedGreaterThan, lhs, rhs),
        BinOp::IGeU => fb.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, lhs, rhs),
        BinOp::FEq => fb.ins().fcmp(FloatCC::Equal, lhs, rhs),
        BinOp::FNe => fb.ins().fcmp(FloatCC::NotEqual, lhs, rhs),
        BinOp::FLt => fb.ins().fcmp(FloatCC::LessThan, lhs, rhs),
        BinOp::FLe => fb.ins().fcmp(FloatCC::LessThanOrEqual, lhs, rhs),
        BinOp::FGt => fb.ins().fcmp(FloatCC::GreaterThan, lhs, rhs),
        BinOp::FGe => fb.ins().fcmp(FloatCC::GreaterThanOrEqual, lhs, rhs),
        BinOp::StrEq | BinOp::StrNe | BinOp::StrConcat => {
            // String ops require a runtime call — wired alongside the
            // ARC runtime in a follow-up step.
            unimplemented!("string ops in mir-codegen")
        }
    }
}

fn lower_cast(
    fb: &mut ClifFnBuilder,
    kind: ilang_mir::CastKind,
    src: Value,
    dst_ty: &MirTy,
    src_mir_ty: &MirTy,
) -> Result<Value, CompileError> {
    use ilang_mir::CastKind;
    let dst_ct = mir_to_clif(dst_ty).ok_or(CompileError::Unsupported("cast to non-clif type"))?;
    Ok(match kind {
        CastKind::IntResize | CastKind::IntSignCross => {
            let src_ty = fb.func.dfg.value_type(src);
            if src_ty.bits() == dst_ct.bits() {
                src
            } else if src_ty.bits() < dst_ct.bits() {
                // Widening: pick uextend for unsigned source (incl.
                // bool / u8 / u16 / u32 / size_t) or for explicit
                // sign-cross casts; sextend for signed widening.
                let use_unsigned = matches!(kind, CastKind::IntSignCross)
                    || src_mir_ty.is_unsigned_int();
                if use_unsigned {
                    fb.ins().uextend(dst_ct, src)
                } else {
                    fb.ins().sextend(dst_ct, src)
                }
            } else {
                fb.ins().ireduce(dst_ct, src)
            }
        }
        CastKind::IntToFloat => {
            if src_mir_ty.is_unsigned_int() {
                fb.ins().fcvt_from_uint(dst_ct, src)
            } else {
                fb.ins().fcvt_from_sint(dst_ct, src)
            }
        }
        CastKind::FloatToInt => {
            // Use the saturating variants — `fcvt_to_sint` /
            // `fcvt_to_uint` trap on out-of-range / NaN inputs,
            // which surfaces as a process SIGILL the user can't
            // catch. The `_sat` forms clamp to the destination
            // type's min/max instead, matching the semantics most
            // ilang code expects (alpha * 255 → 0..255).
            if dst_ty.is_unsigned_int() {
                fb.ins().fcvt_to_uint_sat(dst_ct, src)
            } else {
                fb.ins().fcvt_to_sint_sat(dst_ct, src)
            }
        }
        CastKind::FloatResize => {
            let src_ty = fb.func.dfg.value_type(src);
            if src_ty.bits() < dst_ct.bits() {
                fb.ins().fpromote(dst_ct, src)
            } else {
                fb.ins().fdemote(dst_ct, src)
            }
        }
        CastKind::StrongToWeak | CastKind::PtrCast | CastKind::PtrIntCast => {
            // Pointer reinterprets / weak conversion are identity at
            // the clif level. The REPL slot store / load path also
            // funnels float ↔ i64 round-trips through PtrIntCast; for
            // those we need a real bitcast (or a width-bridging
            // sequence so f32 can flow through an i64 slot). Other
            // mixed-width int↔int cases stay as identity to preserve
            // the legacy "same-rep reinterpret" contract every other
            // call site already depends on.
            let src_ct = fb.func.dfg.value_type(src);
            if src_ct == dst_ct {
                src
            } else if src_ct == types::I64 && dst_ct == types::F64 {
                fb.ins().bitcast(types::F64, MemFlags::new(), src)
            } else if src_ct == types::F64 && dst_ct == types::I64 {
                fb.ins().bitcast(types::I64, MemFlags::new(), src)
            } else if src_ct == types::I64 && dst_ct == types::F32 {
                let narrow = fb.ins().ireduce(types::I32, src);
                fb.ins().bitcast(types::F32, MemFlags::new(), narrow)
            } else if src_ct == types::F32 && dst_ct == types::I64 {
                let bits = fb.ins().bitcast(types::I32, MemFlags::new(), src);
                fb.ins().uextend(types::I64, bits)
            } else {
                src
            }
        }
        CastKind::OptionalWrap => {
            // `T → T?`. For heap-pointer T (object / array / etc.)
            // the bit pattern is reused (null = none). For primitives
            // we'd need to box; the lowerer treats this as identity
            // and the consumer handles unwrap explicitly.
            src
        }
    })
}
