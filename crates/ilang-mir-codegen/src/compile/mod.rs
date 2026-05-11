//! Compile a MIR `Program` into a Cranelift JIT module and invoke
//! the entry function.
//!
//! Currently restricted to programs whose values are all primitive
//! scalars (integers / floats / bool / unit). Heap, ARC, FFI, and
//! virtual dispatch land alongside their MIR features in follow-up
//! steps.

mod abi;
mod cascade;
mod host_array;
mod host_map;
mod host_math;
mod host_misc;
mod host_os;
mod host_raw_mem;
mod host_test;
mod print_kind;

use host_misc::{
    enum_info_lock, fn_name_lock, host_cstr_array_to_strings, host_enum_disc_str, host_identity,
    host_noop, host_optional_missing_stub, host_print_fn, host_print_weak,
    host_string_from_cstr, host_test_live_alloc_bytes, host_test_live_alloc_count,
    host_test_live_string_count, process_symbol_exists, EnumPrintInfo,
};

use cascade::{
    host_release_array, host_release_object, host_release_object_fields, host_release_optional,
    host_retain_array, host_retain_object, host_retain_optional, object_field_table_lock,
    release_by_kind, release_object, release_value_by_kind, retain_by_kind,
};

use host_array::{
    array_header, build_array, host_array_data_ptr, host_array_filter, host_array_for_each,
    host_array_includes, host_array_index_of, host_array_map, host_array_pop, host_array_push,
    host_array_slice, host_c_array_to_array, host_fixed_to_dyn, host_str_split, raw_cstr_bytes,
};
use host_map::{
    host_map_delete, host_map_get, host_map_get_optional, host_map_has, host_map_keys, host_map_new,
    host_map_set, host_map_set_print_kinds, host_map_set_value_kind, host_map_size,
    host_map_values, host_print_map, host_release_map, host_retain_map, ManagedMap,
};

use abi::{
    celem_clif_type_with_enum, clif_signature_for, elem_byte_stride, elem_clif_type, extend_to_i64,
    ireduce_or_pass, reduce_from_i64, struct_chunks, struct_hfa, struct_indirect,
};
use print_kind::{
    format_f64, format_kind_id, kind_tag_of, kind_tag_of_print_kind, print_kind_id,
    print_kind_id_for_print_kind, print_kind_of, PrintKind, KIND_ARRAY, KIND_CLOSURE, KIND_ENUM,
    KIND_MAP, KIND_NONE, KIND_OBJECT, KIND_OPTIONAL, KIND_STR, KIND_TUPLE, PK_ARRAY_I64_SIG,
    PK_BOOL, PK_F32, PK_F64, PK_I16_SIG, PK_I16_UNS, PK_I32_SIG, PK_I32_UNS, PK_I64_SIG,
    PK_I64_UNS, PK_I8_SIG, PK_I8_UNS, PK_OBJECT, PK_OTHER, PK_STR,
};

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

fn declare_unary_i64<M: Module>(
    module: &mut M,
    name: &str,
) -> Result<cranelift_module::FuncId, CompileError> {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(types::I64));
    Ok(module.declare_function(name, Linkage::Import, &sig)?)
}

fn declare_binary_i64<M: Module>(
    module: &mut M,
    name: &str,
) -> Result<cranelift_module::FuncId, CompileError> {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64));
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(types::I64));
    Ok(module.declare_function(name, Linkage::Import, &sig)?)
}

fn declare_ternary_i64<M: Module>(
    module: &mut M,
    name: &str,
) -> Result<cranelift_module::FuncId, CompileError> {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64));
    sig.params.push(AbiParam::new(types::I64));
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(types::I64));
    Ok(module.declare_function(name, Linkage::Import, &sig)?)
}

fn declare_unit_i64<M: Module>(
    module: &mut M,
    name: &str,
) -> Result<cranelift_module::FuncId, CompileError> {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64));
    Ok(module.declare_function(name, Linkage::Import, &sig)?)
}

fn declare_unit_f64<M: Module>(
    module: &mut M,
    name: &str,
) -> Result<cranelift_module::FuncId, CompileError> {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::F64));
    Ok(module.declare_function(name, Linkage::Import, &sig)?)
}

fn declare_unit_void<M: Module>(
    module: &mut M,
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
    struct_: cranelift_module::FuncId,
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
    map_set_val_kind: cranelift_module::FuncId,
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
    enum_unit_get_checked: cranelift_module::FuncId,
    enum_disc_str: cranelift_module::FuncId,
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
    jit_builder.symbol("__mir_alloc", ilang_runtime::__mir_alloc as *const u8);
    jit_builder.symbol("__mir_free", ilang_runtime::__mir_free as *const u8);
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
    jit_builder.symbol("__str_length", ilang_runtime::__str_length as *const u8);
    jit_builder.symbol("__str_concat", ilang_runtime::__str_concat as *const u8);
    jit_builder.symbol("__str_eq", ilang_runtime::__str_eq as *const u8);
    jit_builder.symbol("__int_to_string", ilang_runtime::__int_to_string as *const u8);
    jit_builder.symbol("__bool_to_string", ilang_runtime::__bool_to_string as *const u8);
    jit_builder.symbol("__str_to_upper", ilang_runtime::__str_to_upper as *const u8);
    jit_builder.symbol("__str_to_lower", ilang_runtime::__str_to_lower as *const u8);
    jit_builder.symbol("__str_trim", ilang_runtime::__str_trim as *const u8);
    jit_builder.symbol("__str_includes", ilang_runtime::__str_includes as *const u8);
    jit_builder.symbol("__str_starts_with", ilang_runtime::__str_starts_with as *const u8);
    jit_builder.symbol("__str_ends_with", ilang_runtime::__str_ends_with as *const u8);
    jit_builder.symbol("__str_char_at", ilang_runtime::__str_char_at as *const u8);
    jit_builder.symbol("__str_slice", ilang_runtime::__str_slice as *const u8);
    jit_builder.symbol("__str_replace", ilang_runtime::__str_replace as *const u8);
    jit_builder.symbol("__array_index_of", host_array_index_of as *const u8);
    jit_builder.symbol("__array_includes", host_array_includes as *const u8);
    jit_builder.symbol("__array_push", host_array_push as *const u8);
    jit_builder.symbol("__array_pop", host_array_pop as *const u8);
    jit_builder.symbol("__fixed_to_dyn", ilang_runtime::__fixed_to_dyn as *const u8);
    jit_builder.symbol("__enum_box", ilang_runtime::__enum_box as *const u8);
    jit_builder.symbol("__c_array_to_array", host_c_array_to_array as *const u8);
    jit_builder.symbol("__repl_load_slot", ilang_runtime::__repl_load_slot as *const u8);
    jit_builder.symbol("__repl_store_slot", ilang_runtime::__repl_store_slot as *const u8);
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
    jit_builder.symbol("__array_map", ilang_runtime::__array_map as *const u8);
    jit_builder.symbol("__array_filter", ilang_runtime::__array_filter as *const u8);
    jit_builder.symbol("__array_for_each", ilang_runtime::__array_for_each as *const u8);
    jit_builder.symbol("__array_slice", ilang_runtime::__array_slice as *const u8);
    jit_builder.symbol("__str_split", ilang_runtime::__str_split as *const u8);
    jit_builder.symbol("__virt_dispatch", ilang_runtime::__virt_dispatch as *const u8);
    jit_builder.symbol("__drop_dispatch", ilang_runtime::__drop_dispatch as *const u8);
    jit_builder.symbol("__print_object", ilang_runtime::__print_object as *const u8);
    jit_builder.symbol("__print_struct", ilang_runtime::__print_struct as *const u8);
    jit_builder.symbol("__class_name", ilang_runtime::__class_name as *const u8);
    jit_builder.symbol("__print_weak", ilang_runtime::__print_weak as *const u8);
    jit_builder.symbol("__print_enum", ilang_runtime::__print_enum as *const u8);
    jit_builder.symbol("__print_fn", ilang_runtime::__print_fn as *const u8);
    jit_builder.symbol("__release_object", host_release_object as *const u8);
    jit_builder.symbol("__retain_object", host_retain_object as *const u8);
    jit_builder.symbol("__release_closure", ilang_runtime::__release_closure as *const u8);
    jit_builder.symbol("__retain_closure", ilang_runtime::__retain_closure as *const u8);
    jit_builder.symbol("__release_array", host_release_array as *const u8);
    jit_builder.symbol("__retain_array", host_retain_array as *const u8);
    jit_builder.symbol("__release_optional", host_release_optional as *const u8);
    jit_builder.symbol("__retain_optional", host_retain_optional as *const u8);
    jit_builder.symbol("__release_tuple", ilang_runtime::__release_tuple as *const u8);
    jit_builder.symbol("__retain_tuple", ilang_runtime::__retain_tuple as *const u8);
    jit_builder.symbol("__release_map", host_release_map as *const u8);
    jit_builder.symbol("__retain_map", host_retain_map as *const u8);
    jit_builder.symbol("__release_string", ilang_runtime::__release_string as *const u8);
    jit_builder.symbol("__retain_string", ilang_runtime::__retain_string as *const u8);
    // Always-on memory-tracking helpers exposed through `test.liveAlloc*`
    // / `test.liveStringCount`. Used by the leak-detection fixtures
    // under tests/programs/.
    jit_builder.symbol("test.liveAllocBytes", ilang_runtime::test_live_alloc_bytes as *const u8);
    jit_builder.symbol("test.liveAllocCount", ilang_runtime::test_live_alloc_count as *const u8);
    jit_builder.symbol("test.liveStringCount", ilang_runtime::test_live_string_count as *const u8);
    jit_builder.symbol("__enum_alloc", ilang_runtime::__enum_alloc as *const u8);
    jit_builder.symbol("__release_enum", ilang_runtime::__release_enum as *const u8);
    jit_builder.symbol("__retain_enum", ilang_runtime::__retain_enum as *const u8);
    jit_builder.symbol("__enum_unit_get", ilang_runtime::__enum_unit_get as *const u8);
    jit_builder.symbol(
        "__enum_unit_get_checked",
        ilang_runtime::__enum_unit_get_checked as *const u8,
    );
    jit_builder.symbol("__enum_disc_str", ilang_runtime::__enum_disc_str as *const u8);
    jit_builder.symbol("__map_set_value_kind", host_map_set_value_kind as *const u8);
    jit_builder.symbol("__map_set_print_kinds", host_map_set_print_kinds as *const u8);
    jit_builder.symbol("__print_map", host_print_map as *const u8);
    // FFI marshalling helpers — registered both with their bare names
    // (used inside `@extern(C)` blocks) and qualified names. Strings
    // are NUL-terminated `*const u8` already, so most "C-string"
    // helpers are identity at the bit level.
    jit_builder.symbol("__array_data_ptr", host_array_data_ptr as *const u8);
    jit_builder.symbol("cstrFromString", ilang_runtime::cstr_from_string as *const u8);
    jit_builder.symbol("stringFromCstr", ilang_runtime::string_from_cstr as *const u8);
    jit_builder.symbol("cstrArrayToStrings", ilang_runtime::cstr_array_to_strings as *const u8);
    jit_builder.symbol("freeCstr", ilang_runtime::free_cstr as *const u8);
    jit_builder.symbol("errnoCheck", ilang_runtime::errno_check_i32 as *const u8);
    jit_builder.symbol("errnoCheckI64", ilang_runtime::errno_check_i64 as *const u8);
    jit_builder.symbol("os.errno", ilang_runtime::os_errno as *const u8);
    jit_builder.symbol("os.setErrno", ilang_runtime::os_set_errno as *const u8);
    jit_builder.symbol("os.libLoaded", ilang_runtime::os_lib_loaded as *const u8);
    jit_builder.symbol("os.libLoadError", ilang_runtime::os_lib_load_error as *const u8);
    // Built-in `test.*` runtime — fixture programs use these to
    // self-check. Failures abort the process with exit code 2.
    // Reuse the legacy JIT's full test-extern symbol set (callbacks,
    // by-value structs, sret returns, errno helpers, etc), then
    // override the closure-callback shim with our mir-aware one
    // `test.*` symbols (incl. test.countedFree*) live in
    // `ilang-runtime` now; the explicit `jit_builder.symbol(...)`
    // bindings below pick them up.
    jit_builder.symbol("test.applyI32Cb", ilang_runtime::test_apply_i32_cb as *const u8);
    jit_builder.symbol("test.expect", ilang_runtime::test_expect as *const u8);
    jit_builder.symbol("test.expectStr", ilang_runtime::test_expect_str as *const u8);
    jit_builder.symbol("test.expectBool", ilang_runtime::test_expect_bool as *const u8);
    jit_builder.symbol("test.expectF64", ilang_runtime::test_expect_f64 as *const u8);
    jit_builder.symbol("test.expectTrue", ilang_runtime::test_expect_true as *const u8);
    jit_builder.symbol("test.expectFalse", ilang_runtime::test_expect_false as *const u8);
    jit_builder.symbol("test.fail", ilang_runtime::test_fail as *const u8);
    jit_builder.symbol("test.countedFree", ilang_runtime::test_counted_free as *const u8);
    jit_builder.symbol("test.countedFreeCount", ilang_runtime::test_counted_free_count as *const u8);
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
    jit_builder.symbol("__ilang_panic", ilang_runtime::__ilang_panic as *const u8);
    // Print helpers and `__ilang_panic` live in `ilang-runtime` so JIT
    // and AOT share the same `extern "C"` bodies. We feed JIT the
    // pointer; AOT links against the `.a` facet at build time.
    jit_builder.symbol("__print_int", ilang_runtime::__print_int as *const u8);
    jit_builder.symbol("__print_bool", ilang_runtime::__print_bool as *const u8);
    jit_builder.symbol("__print_f64", ilang_runtime::__print_f64 as *const u8);
    jit_builder.symbol("__print_str", ilang_runtime::__print_str as *const u8);
    jit_builder.symbol("__print_space", ilang_runtime::__print_space as *const u8);
    jit_builder.symbol("__print_newline", ilang_runtime::__print_newline as *const u8);
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
    // through to alternates declared on the same fn. Lives in
    // `ilang-runtime` now so both backends share the registry.
    for f in &prog.functions {
        if matches!(f.kind, ilang_mir::FunctionKind::Extern { .. })
            && f.libs.len() > 1
        {
            let g = ilang_runtime::__register_lib_group_begin();
            for sym in f.libs.iter() {
                let p = ilang_runtime::leak_cstring(sym.as_str().to_string());
                ilang_runtime::__register_lib_group_member(g, p);
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
    let LoweringOutputs { fn_ids, extern_fn_ids, missing_optional_fn_ids: _ } =
        lower_program_into(&mut module, prog, builtins, &class_global, &enum_global)?;
    module
        .finalize_definitions()
        .map_err(CompileError::Module)?;

    // Populate the runtime vtable now that fn addresses are stable.
    // Don't clear — entries are keyed by GLOBAL (class_id, slot) and
    // accumulate so parallel modules coexist without trampling.
    {
        for ((cid, slot), fid) in &vtable_entries {
            if let Some(cl_id) = fn_ids.get(fid) {
                let addr = module.get_finalized_function(*cl_id) as i64;
                ilang_runtime::__register_vtable_entry(*cid as i64, *slot as i64, addr);
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
                    // Mirror into the runtime registry too so AOT-emitted
                    // programs that don't see this JIT process still get
                    // the cascade entries from `__ilang_aot_init`. The
                    // JIT path uses the richer `PrintKind` table above for
                    // its in-process cascade; both backends populate, but
                    // only the JIT host reads the local PrintKind copy.
                    ilang_runtime::__register_object_field(
                        global_cid(class.id.0) as i64,
                        off,
                        kind_tag_of_print_kind(&kind),
                    );
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
                ilang_runtime::__register_class_size(
                    global_cid(class.id.0) as i64,
                    size,
                );
            }
        }
    }
    // Populate the class-print-info registry in `ilang-runtime` —
    // `__print_object` walks an object's fields via the runtime's
    // copy. AOT mirrors the same registrations from
    // `__ilang_aot_init` using data-symbol-backed strings.
    for class in &prog.classes {
        let gcid = global_cid(class.id.0) as i64;
        let name_ptr = ilang_runtime::leak_cstring(class.name.as_str().to_string());
        ilang_runtime::__register_class_print_name(gcid, name_ptr);
        let is_struct = matches!(
            class.repr,
            ilang_mir::ClassRepr::CRepr
                | ilang_mir::ClassRepr::CPacked
                | ilang_mir::ClassRepr::CUnion
        );
        for (i, f) in class.fields.iter().enumerate() {
            let pk = print_kind_id(&f.ty);
            let fname_ptr = ilang_runtime::leak_cstring(f.name.as_str().to_string());
            ilang_runtime::__register_class_print_field(gcid, i as i64, fname_ptr, pk);
            if is_struct {
                // CRepr / CPacked / CUnion: also populate the struct
                // print registry with each field's natural byte
                // offset so `__print_struct` can read with C layout.
                // Bit-field fields don't have a byte slot of their
                // own — skip them rather than report a fake offset.
                if f.bit_field.is_some() {
                    continue;
                }
                let off = class
                    .c_field_offsets
                    .get(i)
                    .copied()
                    .unwrap_or(0);
                // Inline nested struct: pass the nested global cid so
                // the formatter recurses on its inlined bytes rather
                // than misreading the cell as a heap pointer.
                let nested_cid: i64 = if let MirTy::Object(nc) = &f.ty {
                    let nested = &prog.classes[nc.0 as usize];
                    if matches!(
                        nested.repr,
                        ilang_mir::ClassRepr::CRepr
                            | ilang_mir::ClassRepr::CPacked
                            | ilang_mir::ClassRepr::CUnion
                    ) {
                        global_cid(nc.0) as i64
                    } else {
                        0
                    }
                } else {
                    0
                };
                let fname_ptr = ilang_runtime::leak_cstring(f.name.as_str().to_string());
                ilang_runtime::__register_struct_print_field(
                    gcid, i as i64, fname_ptr, pk, off, nested_cid,
                );
            }
        }
    }
    // Populate enum-print-info registry — host_print_enum walks
    // enum tag → variant name + payload kinds. Also mirror per-
    // variant payload kinds into the runtime's smaller
    // `ENUM_PAYLOAD_KINDS` registry that drives `__release_enum`'s
    // cascade (so AOT-built programs see the same release cascade
    // through `__register_enum_payload_kind` calls from
    // `__ilang_aot_init`).
    // Enum print + cascade registries both live in `ilang-runtime`;
    // mirror the JIT-side computed `(name, variants, payload kinds)`
    // into both at once. The JIT-local `ENUM_INFO` keeps the
    // string-repr lookup it still needs for `enum-as-string` casts.
    {
        let mut t = enum_info_lock().lock().expect("enum info poisoned");
        for e in &prog.enums {
            let global_id = global_eid(e.id.0);
            let name_ptr = ilang_runtime::leak_cstring(e.name.as_str().to_string());
            ilang_runtime::__register_enum_print_name(global_id as i64, name_ptr);
            let mut variants: HashMap<i64, (String, Vec<PrintKind>)> = HashMap::new();
            let is_str_repr = matches!(e.repr, MirTy::Str);
            let mut str_repr: HashMap<i64, String> = HashMap::new();
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
                let vname_ptr = ilang_runtime::leak_cstring(v.name.as_str().to_string());
                ilang_runtime::__register_enum_print_variant_name(
                    global_id as i64,
                    v.discriminant,
                    vname_ptr,
                );
                for (i, k) in kinds.iter().enumerate() {
                    // Cascade tag (KIND_*) for release cascade.
                    let cascade_tag = kind_tag_of_print_kind(k);
                    if cascade_tag != KIND_NONE {
                        ilang_runtime::__register_enum_payload_kind(
                            global_id as i64,
                            v.discriminant,
                            i as i64,
                            cascade_tag,
                        );
                    }
                    // Print tag (PK_*) for `__print_enum`.
                    let pk = print_kind_id_for_print_kind(k);
                    ilang_runtime::__register_enum_print_variant_payload_pk(
                        global_id as i64,
                        v.discriminant,
                        i as i64,
                        pk,
                    );
                }
                variants.insert(v.discriminant, (v.name.as_str().to_string(), kinds));
                if is_str_repr {
                    if let Some(s) = v.discriminant_str.as_ref() {
                        str_repr.insert(v.discriminant, s.clone());
                        // Mirror into the runtime registry that
                        // `__enum_disc_str` reads so AOT-built
                        // programs see the same mapping.
                        let sp = ilang_runtime::leak_cstring(s.clone());
                        ilang_runtime::__register_enum_disc_str(
                            global_id as i64,
                            v.discriminant,
                            sp,
                        );
                    }
                }
            }
            t.insert(
                global_id,
                EnumPrintInfo {
                    name: e.name.as_str().to_string(),
                    variants,
                    str_repr: if is_str_repr { Some(str_repr) } else { None },
                },
            );
        }
    }
    // Populate closure capture / size tables in the runtime crate —
    // `ilang_runtime::__release_closure` walks the captures table at
    // rc-zero. AOT mirrors these registrations from
    // `__ilang_aot_init`; both backends share one map.
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
        for (i, cap) in env.captures.iter().enumerate() {
            if cap.is_cell {
                // Cells are 1-element arrays — leak for now.
                continue;
            }
            let tag = kind_tag_of(&cap.ty);
            if tag == KIND_NONE {
                continue;
            }
            let off = 16 + (i as i64) * 8;
            ilang_runtime::__register_closure_capture(addr, off, tag);
        }
        let total_size = (2 + env.captures.len() as i64) * 8;
        ilang_runtime::__register_closure_size(addr, total_size);
    }
    // Populate fn-name registry — `__print_fn` looks up the fn
    // address (closure[0]) and prints "<fn NAME>" / "<fn>". Lives
    // in `ilang-runtime` so AOT-built programs see the same table.
    // Skip extern fns (no compiled body) and synthetic names.
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
                let name_ptr = ilang_runtime::leak_cstring(plain.to_string());
                ilang_runtime::__register_fn_name(addr, name_ptr);
            }
        }
    }
    // Populate the class-id → drop fn registry. `drop_fn` is set by
    // the lowering whenever a class declares `deinit`. Subclasses
    // that don't redefine deinit inherit the parent's via the
    // method table — read it back from the lowered class.
    for class in &prog.classes {
        if class.drop_fn.0 != u32::MAX {
            if let Some(cl_id) = fn_ids.get(&class.drop_fn) {
                let addr = module.get_finalized_function(*cl_id) as i64;
                ilang_runtime::__register_drop(global_cid(class.id.0) as i64, addr);
            }
        }
    }

    let entry_fn = &prog.functions[prog.entry.0 as usize];
    let entry_ret = entry_fn.ret.clone();
    let entry = *fn_ids.get(&prog.entry).expect("entry registered");

    Ok(Compiled { module, entry, entry_ret })
}
pub(crate) struct LoweringOutputs {
    pub fn_ids: HashMap<FuncId, cranelift_module::FuncId>,
    pub extern_fn_ids: std::collections::HashSet<FuncId>,
    /// `@optional` extern fns whose every `@lib(...)` failed to
    /// probe — declared `Linkage::Local` so the caller can attach an
    /// abort-stub body before `module.finalize`.
    pub missing_optional_fn_ids: std::collections::HashSet<FuncId>,
}

pub(crate) fn lower_program_into<M: Module>(
    module: &mut M,
    prog: &Program,
    builtins: &[BuiltinDecl],
    class_global: &[u32],
    enum_global: &[u32],
) -> Result<LoweringOutputs, CompileError> {
    lower_program_into_with_missing(
        module,
        prog,
        builtins,
        class_global,
        enum_global,
        &std::collections::HashSet::new(),
    )
}

pub(crate) fn lower_program_into_with_missing<M: Module>(
    module: &mut M,
    prog: &Program,
    builtins: &[BuiltinDecl],
    class_global: &[u32],
    enum_global: &[u32],
    missing_optional_syms: &std::collections::HashSet<String>,
) -> Result<LoweringOutputs, CompileError> {
    // For each local class id, its GLOBAL class id if the class is a
    // CRepr / CPacked / CUnion struct (`@extern(C)` block, no rc
    // header, C-natural alignment) — `-1` otherwise. The print path
    // routes struct-typed `MirTy::Object` to `__print_struct(global,
    // ptr)` instead of `__print_object(ptr)`, which would misread the
    // first field as a class_id header.
    let class_struct_global: Vec<i64> = prog
        .classes
        .iter()
        .enumerate()
        .map(|(i, c)| {
            if matches!(
                c.repr,
                ilang_mir::ClassRepr::CRepr
                    | ilang_mir::ClassRepr::CPacked
                    | ilang_mir::ClassRepr::CUnion
            ) {
                class_global[i] as i64
            } else {
                -1
            }
        })
        .collect();
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
    let map_size_id = declare_unary_i64(module, "__map_size")?;
    let map_delete_id = declare_binary_i64(module, "__map_delete")?;
    let map_keys_id = declare_unary_i64(module, "__map_keys")?;
    let map_values_id = declare_unary_i64(module, "__map_values")?;
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
        length: declare_unary_i64(module, "__str_length")?,
        concat: declare_binary_i64(module, "__str_concat")?,
        eq: declare_binary_i64(module, "__str_eq")?,
        int_to_string: declare_unary_i64(module, "__int_to_string")?,
        bool_to_string: declare_unary_i64(module, "__bool_to_string")?,
        to_upper: declare_unary_i64(module, "__str_to_upper")?,
        to_lower: declare_unary_i64(module, "__str_to_lower")?,
        trim: declare_unary_i64(module, "__str_trim")?,
        includes: declare_binary_i64(module, "__str_includes")?,
        starts_with: declare_binary_i64(module, "__str_starts_with")?,
        ends_with: declare_binary_i64(module, "__str_ends_with")?,
        char_at: declare_binary_i64(module, "__str_char_at")?,
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
        array_index_of: declare_binary_i64(module, "__array_index_of")?,
        array_includes: declare_binary_i64(module, "__array_includes")?,
        array_push: {
            let mut sig = module.make_signature();
            sig.params.push(AbiParam::new(types::I64));
            sig.params.push(AbiParam::new(types::I64));
            module.declare_function("__array_push", Linkage::Import, &sig)?
        },
        array_pop: declare_unary_i64(module, "__array_pop")?,
        array_map: declare_ternary_i64(module, "__array_map")?,
        array_filter: declare_binary_i64(module, "__array_filter")?,
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
        str_split: declare_binary_i64(module, "__str_split")?,
        virt_dispatch: declare_binary_i64(module, "__virt_dispatch")?,
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
    let panic_fn_id = declare_unit_i64(module, "__ilang_panic")?;
    let drop_dispatch_id = declare_unary_i64(module, "__drop_dispatch")?;
    let print_object_id = declare_unit_i64(module, "__print_object")?;
    let print_struct_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        module.declare_function("__print_struct", Linkage::Import, &sig)?
    };
    let print_fn_id = declare_unit_i64(module, "__print_fn")?;
    let release_obj_id = declare_unit_i64(module, "__release_object")?;
    let retain_obj_id = declare_unit_i64(module, "__retain_object")?;
    let release_closure_id = declare_unit_i64(module, "__release_closure")?;
    let retain_closure_id = declare_unit_i64(module, "__retain_closure")?;
    let release_array_id = declare_unit_i64(module, "__release_array")?;
    let retain_array_id = declare_unit_i64(module, "__retain_array")?;
    let release_optional_id = declare_unit_i64(module, "__release_optional")?;
    let retain_optional_id = declare_unit_i64(module, "__retain_optional")?;
    let release_tuple_id = declare_unit_i64(module, "__release_tuple")?;
    let retain_tuple_id = declare_unit_i64(module, "__retain_tuple")?;
    let release_map_id = declare_unit_i64(module, "__release_map")?;
    let retain_map_id = declare_unit_i64(module, "__retain_map")?;
    let release_string_id = declare_unit_i64(module, "__release_string")?;
    let retain_string_id = declare_unit_i64(module, "__retain_string")?;
    let enum_unit_get_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        module.declare_function("__enum_unit_get", Linkage::Import, &sig)?
    };
    let enum_unit_get_checked_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        module.declare_function("__enum_unit_get_checked", Linkage::Import, &sig)?
    };
    let enum_disc_str_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        module.declare_function("__enum_disc_str", Linkage::Import, &sig)?
    };
    let enum_alloc_id = declare_ternary_i64(module, "__enum_alloc")?;
    let release_enum_id = declare_unit_i64(module, "__release_enum")?;
    let retain_enum_id = declare_unit_i64(module, "__retain_enum")?;
    let map_set_val_kind_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        module.declare_function("__map_set_value_kind", Linkage::Import, &sig)?
    };
    let map_set_print_kinds_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        module.declare_function("__map_set_print_kinds", Linkage::Import, &sig)?
    };
    let print_map_id = declare_unit_i64(module, "__print_map")?;
    let class_name_id = declare_unary_i64(module, "__class_name")?;
    let print_weak_id = declare_unit_i64(module, "__print_weak")?;
    let print_enum_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        module.declare_function("__print_enum", Linkage::Import, &sig)?
    };
    let print_ids = PrintIds {
        int: declare_unit_i64(module, "__print_int")?,
        bool_: declare_unit_i64(module, "__print_bool")?,
        f64_: declare_unit_f64(module, "__print_f64")?,
        str_: declare_unit_i64(module, "__print_str")?,
        space: declare_unit_void(module, "__print_space")?,
        newline: declare_unit_void(module, "__print_newline")?,
        object: print_object_id,
        struct_: print_struct_id,
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
                        // Align=8 — the `[ i64 len | bytes | \0 ]`
                        // layout reads `*((ptr - 8) as *const i64)`
                        // for the length. Without explicit alignment
                        // Cranelift packs data segments at byte
                        // alignment, so the length read trips Rust's
                        // misaligned-pointer check at runtime.
                        desc.set_align(8);
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
        desc.set_align(8);
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
    let mut missing_optional_fn_ids: std::collections::HashSet<FuncId> =
        std::collections::HashSet::new();
    for (idx, func) in prog.functions.iter().enumerate() {
        let mid = FuncId(idx as u32);
        let sig = clif_signature_for(&*module, func, prog)?;
        // For `@extern(C) @symbol("foo") fn bar(...)`, declare under
        // the C-side name `foo` so dlsym resolves correctly while
        // ilang-side calls still go through this FuncId via `bar`.
        let symbol_name: &str = if let Some(c) = func.c_symbol {
            c.as_str()
        } else {
            func.name.as_str()
        };
        let linkage = if matches!(func.kind, ilang_mir::FunctionKind::Extern { .. }) {
            extern_fn_ids.insert(mid);
            if missing_optional_syms.contains(symbol_name) {
                // Optional extern whose libs all failed to probe at
                // AOT build time. Caller will define a stub body
                // before finalize so the link step doesn't see an
                // unresolved Import.
                missing_optional_fn_ids.insert(mid);
                Linkage::Local
            } else {
                Linkage::Import
            }
        } else {
            Linkage::Local
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

        // Lower into ctx.func; we need module to declare imports
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
                map_set_val_kind: map_set_val_kind_id,
                map_set_print_kinds: map_set_print_kinds_id,
                print_map: print_map_id,
                class_name: class_name_id,
                mir_free: free_id,
                release_string: release_string_id,
                retain_string: retain_string_id,
                enum_unit_get: enum_unit_get_id,
                enum_unit_get_checked: enum_unit_get_checked_id,
                enum_disc_str: enum_disc_str_id,
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
            let stack_local =
                if std::env::var_os("ILANG_NO_STACK_PROMOTE").is_some() {
                    std::collections::HashSet::new()
                } else {
                    let set = ilang_mir::passes::escape_object::analyze_function(prog, idx);
                    if std::env::var_os("ILANG_DUMP_STACK_PROMOTE").is_some() && !set.is_empty() {
                        eprintln!(
                            "stack_promote fn={} values={:?}",
                            func.name.as_str(),
                            set.iter().map(|v| v.0).collect::<Vec<_>>()
                        );
                    }
                    set
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
                module,
                prog,
                &class_global,
                &enum_global,
                &class_struct_global,
                &stack_local,
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
    Ok(LoweringOutputs { fn_ids, extern_fn_ids, missing_optional_fn_ids })
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

pub(crate) fn alloc_global_class_id() -> u32 {
    NEXT_GLOBAL_CLASS_ID.fetch_add(1, AtomicOrdering::Relaxed)
}

pub(crate) fn alloc_global_enum_id() -> u32 {
    NEXT_GLOBAL_ENUM_ID.fetch_add(1, AtomicOrdering::Relaxed)
}

// VTABLE / DROP_TABLE moved to `ilang-runtime` (`__register_vtable_entry`
// / `__register_drop` populate; `__virt_dispatch` / `__drop_dispatch`
// read). Both backends now funnel through that single in-process map.

// `OBJECT_FIELD_TABLE` + `object_field_table_lock` moved to
// `compile/cascade.rs` alongside the release helpers that consume it.
// Class size table lives in `ilang-runtime` (`__register_class_size`);
// JIT and AOT funnel through it via `__release_object`.

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





// String runtime helpers. Each ilang string lives on the heap as
//   [ i64 length ][ UTF-8 bytes ][ \0 ]
// and the user-visible pointer points at the first UTF-8 byte. The
// length prefix lets reads survive embedded NULs (e.g. `"a\0b"` has
// length 3); the trailing NUL keeps cstr-style C interop working
// (snprintf etc. read up to the first NUL, which is a documented
// truncation if the user puts NULs inside the string).
use ilang_runtime::{cstr_bytes, cstr_to_str, leak_cstring};


use host_raw_mem::*;

/// Box a raw discriminant value into a unit-variant enum heap cell.
/// Layout matches `Inst::NewEnum` for unit variants: 8 B containing
/// the tag at offset 0, no payload. Used by the integer→enum
/// REPL host-slot storage moved to `ilang-runtime`
/// (`__repl_load_slot` / `__repl_store_slot`); both backends share
/// the same in-process Vec via the JIT symbol map and the AOT
/// import. `reset_repl_slots` re-exports the runtime hook for the
/// REPL session bootstrap.
pub use ilang_runtime::reset_repl_slots;

// `__enum_box`, `__enum_unit_get`, `__enum_unit_get_checked`,
// `__enum_alloc`, `__retain_enum`, `__release_enum` all live in
// `ilang-runtime` (along with `ENUM_REGISTRY` / `ENUM_PAYLOAD_KINDS`
// / `ENUM_UNIT_CACHE`). Both backends feed
// `__register_enum_payload_kind` from their populate steps so the
// cascade walks the same kind tags either way.


use host_math::*;
use host_os::*;
use host_test::*;





fn lower_function<M: Module>(
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
    module: &mut M,
    prog: &Program,
    class_global: &[u32],
    enum_global: &[u32],
    class_struct_global: &[i64],
    stack_local: &std::collections::HashSet<ValueId>,
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
                class_struct_global,
                stack_local,
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

fn lower_inst<M: Module>(
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
    module: &mut M,
    locals: &[Variable],
    prog: &Program,
    env_value: Value,
    class_global: &[u32],
    enum_global: &[u32],
    class_struct_global: &[i64],
    stack_local: &std::collections::HashSet<ValueId>,
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
                        emit_print_value(fb, module, print_ids, print_lits, &aty, av, enum_global, class_struct_global);
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
            // Same rationale as the matching `Release` branch: a
            // stack-promoted object has no rc to bump.
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
            let total_bytes = OBJECT_HEADER_BYTES as i64 + n_fields * 8;
            // Stack-promotion fast path: escape analysis has cleared
            // this `dst`, so allocate a cranelift StackSlot inside
            // the current function frame instead of going through
            // __mir_alloc. Field offsets and LoadField / StoreField
            // / VirtCall layouts stay identical (header + n*8). The
            // matching `Retain` / `Release` calls are no-op'd below
            // so the stack memory's lifetime is the function frame's.
            let ptr = if stack_local.contains(dst) {
                let slot = fb.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    total_bytes as u32,
                    3,
                ));
                let p = fb.ins().stack_addr(types::I64, slot, 0);
                // Zero the slot's bytes — heap alloc zeros via
                // __mir_alloc; we keep the same invariant so any
                // primitive field read before its first write sees
                // 0 instead of stack garbage.
                let zero = fb.ins().iconst(types::I64, 0);
                let mut off = 0;
                while off < total_bytes {
                    fb.ins().store(MemFlags::trusted(), zero, p, off as i32);
                    off += 8;
                }
                p
            } else {
                let size = fb.ins().iconst(types::I64, total_bytes);
                let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
                let alloc_call = fb.ins().call(alloc_ref, &[size]);
                fb.inst_results(alloc_call)[0]
            };
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
            // Tag the map's value-side runtime kind so host_map_set
            // can retain on insert and host_release_map can cascade-
            // release on drop, for any heap-typed value (Object,
            // String, Array, Tuple, Optional, Map, Closure, Enum).
            let val_kind = kind_tag_of(val);
            if val_kind != KIND_NONE {
                let mark_ref =
                    module.declare_func_in_func(panic_aux.map_set_val_kind, fb.func);
                let kind_v = fb.ins().iconst(types::I64, val_kind);
                fb.ins().call(mark_ref, &[map_ptr, kind_v]);
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
        Inst::EnumDiscStr { dst, enum_id, value } => {
            // `enum-as-string` cast for `: string`-repr enums.
            // Load the box's tag (variant index), then call
            // `__enum_disc_str(global, tag)` to get a fresh
            // `StringRc *` with the variant's declared
            // discriminant string.
            let p = vmap[value];
            let tag = fb.ins().load(types::I64, MemFlags::trusted(), p, 0);
            let global = enum_global[enum_id.0 as usize] as i64;
            let global_v = fb.ins().iconst(types::I64, global);
            let f = module.declare_func_in_func(panic_aux.enum_disc_str, fb.func);
            let call = fb.ins().call(f, &[global_v, tag]);
            let v = fb.inst_results(call)[0];
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
                // Unit-only enum field: read the discriminant at the
                // repr's natural width, then look up the cached unit
                // cell so downstream `EnumTag` / `match` see a
                // proper heap-box pointer. The lookup aborts if the
                // value the C side wrote isn't a declared variant —
                // matches the `repr(C)` panic-on-unknown contract
                // discussed in the language design notes.
                if let MirTy::Enum(eid) = &dst_ty_mir {
                    let layout = &prog.enums[eid.0 as usize];
                    let unit_only = layout
                        .variants
                        .iter()
                        .all(|v| matches!(v.payload, ilang_mir::VariantPayload::Unit));
                    if unit_only {
                        let repr_ct = elem_clif_type(&layout.repr).unwrap_or(types::I64);
                        let raw = fb.ins().load(repr_ct, MemFlags::trusted(), obj_v, c_off as i32);
                        let disc_i64 = if repr_ct == types::I64 {
                            raw
                        } else if layout.repr.is_signed_int() {
                            fb.ins().sextend(types::I64, raw)
                        } else {
                            fb.ins().uextend(types::I64, raw)
                        };
                        let global = enum_global[eid.0 as usize] as i64;
                        let global_v = fb.ins().iconst(types::I64, global);
                        let f = module.declare_func_in_func(
                            panic_aux.enum_unit_get_checked,
                            fb.func,
                        );
                        let call = fb.ins().call(f, &[global_v, disc_i64]);
                        let v = fb.inst_results(call)[0];
                        vmap.insert(*dst, v);
                        return Ok(());
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
                // Unit-only enum field: the SSA value is a heap-box
                // pointer; the C struct slot wants the underlying
                // discriminant. Load tag from the box (offset 0) and
                // narrow to the field's repr width before storing.
                let raw = if let MirTy::Enum(eid) = &val_ty_mir {
                    let layout = &prog.enums[eid.0 as usize];
                    let unit_only = layout
                        .variants
                        .iter()
                        .all(|v| matches!(v.payload, ilang_mir::VariantPayload::Unit));
                    if unit_only {
                        fb.ins().load(types::I64, MemFlags::trusted(), raw, 0)
                    } else {
                        raw
                    }
                } else {
                    raw
                };
                match celem_clif_type_with_enum(prog, &val_ty_mir) {
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
fn emit_print_lit<M: Module>(
    fb: &mut ClifFnBuilder,
    module: &mut M,
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
fn emit_print_value<M: Module>(
    fb: &mut ClifFnBuilder,
    module: &mut M,
    print_ids: PrintIds,
    print_lits: PrintLits,
    ty: &MirTy,
    av: Value,
    enum_global: &[u32],
    class_struct_global: &[i64],
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
            emit_print_value(fb, module, print_ids, print_lits, inner, inner_v, enum_global, class_struct_global);
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
                emit_print_value(fb, module, print_ids, print_lits, ity, elem_v, enum_global, class_struct_global);
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
            emit_print_value(fb, module, print_ids, print_lits, elem, elem_v, enum_global, class_struct_global);
            // i = i + 1
            let one = fb.ins().iconst(types::I64, 1);
            let i_next = fb.ins().iadd(i_arg, one);
            fb.ins().jump(header, [i_next.into()].iter());

            fb.seal_block(header);
            fb.switch_to_block(exit_blk);
            fb.seal_block(exit_blk);
            emit_print_lit(fb, module, print_ids.str_, print_lits.close_bracket);
        }
        MirTy::Object(cid) => {
            let sg = class_struct_global
                .get(cid.0 as usize)
                .copied()
                .unwrap_or(-1);
            if sg >= 0 {
                let cid_v = fb.ins().iconst(types::I64, sg);
                let r = module.declare_func_in_func(print_ids.struct_, fb.func);
                fb.ins().call(r, &[cid_v, av]);
            } else {
                let r = module.declare_func_in_func(print_ids.object, fb.func);
                fb.ins().call(r, &[av]);
            }
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
fn emit_panic_if<M: Module>(
    fb: &mut ClifFnBuilder,
    module: &mut M,
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

pub(crate) fn lower_binop(fb: &mut ClifFnBuilder, op: BinOp, lhs: Value, rhs: Value) -> Value {
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
