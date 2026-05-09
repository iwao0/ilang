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
    BinOp, ClassId, FieldId, FuncId, FuncRef, Function as MirFunction, Inst, MirConst, MirTy,
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

    // Pre-build a per-(class_id, slot) → method-fn-id map from the
    // MIR. The actual function addresses are filled in after
    // `finalize_definitions()` and exposed to JIT code via the
    // `__virt_dispatch` host helper.
    let mut vtable_entries: HashMap<(u32, u32), FuncId> = HashMap::new();
    for class in &prog.classes {
        for m in &class.methods {
            if let Some(slot) = m.slot {
                vtable_entries.insert((class.id.0, slot.0), m.func);
            }
        }
    }

    let mut jit_builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    // Always-available allocator. Allocates `size` bytes (zero-init)
    // via Rust's `Vec<u8>` and leaks the pointer. The MIR codegen's
    // ARC step is what eventually frees it; until then it's a small
    // intentional leak that's fine for short-running test programs.
    jit_builder.symbol("__mir_alloc", host_mir_alloc as *const u8);
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
    jit_builder.symbol("__map_set_object_value", host_map_set_object_value as *const u8);
    jit_builder.symbol("__map_set_print_kinds", host_map_set_print_kinds as *const u8);
    jit_builder.symbol("__print_map", host_print_map as *const u8);
    // FFI marshalling helpers — registered both with their bare names
    // (used inside `@extern(C)` blocks) and qualified names. Strings
    // are NUL-terminated `*const u8` already, so most "C-string"
    // helpers are identity at the bit level.
    jit_builder.symbol("__array_data_ptr", host_array_data_ptr as *const u8);
    jit_builder.symbol("cstrFromString", host_identity as *const u8);
    jit_builder.symbol("stringFromCstr", host_identity as *const u8);
    jit_builder.symbol("cstrArrayToStrings", host_cstr_array_to_strings as *const u8);
    jit_builder.symbol("freeCstr", host_noop as *const u8);
    jit_builder.symbol("errnoCheck", host_errno_check_i32 as *const u8);
    jit_builder.symbol("errnoCheckI64", host_errno_check_i64 as *const u8);
    jit_builder.symbol("os.errno", host_os_errno as *const u8);
    jit_builder.symbol("os.setErrno", host_os_set_errno as *const u8);
    // Built-in `test.*` runtime — fixture programs use these to
    // self-check. Failures abort the process with exit code 2.
    jit_builder.symbol("test.applyI32Cb", host_test_apply_i32_cb as *const u8);
    // Reuse the legacy JIT's full test-extern symbol set (callbacks,
    // by-value structs, sret returns, errno helpers, etc). Layout-
    // mismatched ones (StringRc payload) won't be invoked by the
    // mir-jit fixtures that rely on those.
    ilang_codegen::test_externs::register_test_symbols(&mut jit_builder);
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
    let mut module = JITModule::new(jit_builder);

    // Declare the alloc builtin so NewObject can call it.
    let alloc_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        module.declare_function("__mir_alloc", Linkage::Import, &sig)?
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
        decl_unary("cstrFromString", false)?;
        decl_unary("stringFromCstr", false)?;
        decl_unary("cstrArrayToStrings", false)?;
        decl_unary("freeCstr", true)?;
        decl_unary("errnoCheck", false)?;
        decl_unary("errnoCheckI64", false)?;
        // os.errno / os.setErrno are declared by the user's @extern(C)
        // block (the `os` stdlib); we just register the host symbols.
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
        array_map: declare_binary_i64(&mut module, "__array_map")?,
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
    // Cranelift data symbol with NUL-terminated UTF-8 bytes.
    let mut string_data: HashMap<Symbol, DataId> = HashMap::new();
    let mut next_str_id: u32 = 0;
    for f in &prog.functions {
        for blk in &f.blocks {
            for inst in &blk.insts {
                if let Inst::Const { value: MirConst::Str(s), .. } = inst {
                    if !string_data.contains_key(s) {
                        let mut bytes: Vec<u8> = s.as_str().as_bytes().to_vec();
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
    let mut declare_msg = |name: &str, text: &str| -> Result<DataId, CompileError> {
        let mut bytes: Vec<u8> = text.as_bytes().to_vec();
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
        let sig = clif_signature_for(&module, func)?;
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
            )?;
            fb.finalize();
        }

        if let Err(e) = module.define_function(cid, &mut ctx) {
            // Surface the function name + clif IR for easier debugging.
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
    {
        let mut vt = vtable_lock().lock().expect("vtable poisoned");
        vt.clear();
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
        let mut t = object_field_table_lock()
            .lock()
            .expect("field table poisoned");
        t.clear();
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
            t.insert(class.id.0, entries);
        }
    }
    // Populate the class-print-info registry — host_print_object
    // walks an object's fields by class id.
    {
        let mut info_map = class_info_lock().lock().expect("class info poisoned");
        info_map.clear();
        for class in &prog.classes {
            let fields: Vec<(String, PrintKind)> = class
                .fields
                .iter()
                .map(|f| (f.name.as_str().to_string(), print_kind_of(&f.ty)))
                .collect();
            info_map.insert(
                class.id.0,
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
        t.clear();
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
                e.id.0,
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
        t.clear();
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
        dt.clear();
        for class in &prog.classes {
            if class.drop_fn.0 != u32::MAX {
                if let Some(cl_id) = fn_ids.get(&class.drop_fn) {
                    let addr = module.get_finalized_function(*cl_id) as i64;
                    dt.insert(class.id.0, addr);
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
) -> Value {
    // Collect target + every descendant via a single hierarchy scan.
    let mut accept: Vec<i64> = vec![target.0 as i64];
    // Multi-pass: any class whose parent is already in `accept` joins.
    loop {
        let before = accept.len();
        for c in &prog.classes {
            if let Some(p) = c.parent {
                if !accept.contains(&(c.id.0 as i64)) && accept.contains(&(p.0 as i64)) {
                    accept.push(c.id.0 as i64);
                }
            }
        }
        if accept.len() == before {
            break;
        }
    }
    let mut result: Option<Value> = None;
    for cid in accept {
        let lit = fb.ins().iconst(types::I64, cid);
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

use std::sync::Mutex;
use std::sync::OnceLock;

/// Runtime vtable: (class_id, slot) → fn pointer (i64). Populated
/// after `module.finalize_definitions()` for the latest compile.
/// Subsequent `compile_program` calls overwrite. Single-threaded
/// usage for now.
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
    let mask = unsafe { *((base + 8) as *const i64) };
    if mask == 0 {
        return;
    }
    let mut m = mask as u64;
    let mut i: i64 = 0;
    while m != 0 {
        if m & 1 != 0 {
            let elem = unsafe { *((tup_ptr + i * 8) as *const i64) };
            release_object(elem);
        }
        m >>= 1;
        i += 1;
    }
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
    if tag == 1 {
        let inner = unsafe { *(opt_ptr as *const i64) };
        release_object(inner);
    }
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
    if tag == 1 {
        let len = unsafe { *(arr_ptr as *const i64) };
        let data_ptr = unsafe { *((arr_ptr + 16) as *const i64) };
        for i in 0..len {
            let elem = unsafe { *((data_ptr + i * 8) as *const i64) };
            release_object(elem);
        }
    }
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
    let mut bytes: Vec<u8> = base.into_bytes();
    bytes.push(0);
    let bx = bytes.into_boxed_slice();
    let ptr = bx.as_ptr() as i64;
    Box::leak(bx);
    ptr
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

extern "C" fn host_mir_alloc(size: i64) -> i64 {
    let n = size as usize;
    let mut v: Vec<u8> = vec![0; n];
    let ptr = v.as_mut_ptr() as i64;
    std::mem::forget(v);
    ptr
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
            // Re-emit a leaked NUL-terminated copy so callers reading
            // back keys via host_map_keys see a stable C-string ptr.
            let mut bytes: Vec<u8> = s.as_bytes().to_vec();
            bytes.push(0);
            let bx = bytes.into_boxed_slice();
            let ptr = bx.as_ptr() as i64;
            Box::leak(bx);
            ptr
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
    m.inner.insert(mk, value);
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

// String runtime helpers — operate on NUL-terminated `*const u8`
// pointers. Returned strings are leaked Rust allocations.
unsafe fn cstr_bytes<'a>(p: i64) -> &'a [u8] { unsafe {
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

fn leak_cstring(s: String) -> i64 {
    let mut bytes = s.into_bytes();
    bytes.push(0);
    let boxed = bytes.into_boxed_slice();
    let ptr = boxed.as_ptr() as i64;
    Box::leak(boxed);
    ptr
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

extern "C" fn host_array_push(arr: i64, value: i64) {
    if arr == 0 {
        return;
    }
    unsafe {
        let h = arr as *mut i64;
        let len = *h;
        let cap = *h.add(1);
        let data = *h.add(2);
        if len < cap {
            *((data + len * 8) as *mut i64) = value;
            *h = len + 1;
        } else {
            // Grow: alloc new buffer (2x capacity, min 4), copy, swap.
            let new_cap = (cap * 2).max(4);
            let new_data = host_mir_alloc(new_cap * 8);
            std::ptr::copy_nonoverlapping(
                data as *const u8,
                new_data as *mut u8,
                (len as usize) * 8,
            );
            *((new_data + len * 8) as *mut i64) = value;
            *h = len + 1;
            *h.add(1) = new_cap;
            *h.add(2) = new_data;
            // Old `data` allocation leaks until ARC lands.
        }
    }
}

/// Construct a new array (using the codegen's 24-byte header layout)
/// from an i64 slice. Returns the i64 header pointer.
fn build_array(items: &[i64]) -> i64 {
    let cap = items.len().max(4);
    let header = host_mir_alloc(24);
    let data = host_mir_alloc((cap * 8) as i64);
    unsafe {
        let h = header as *mut i64;
        *h = items.len() as i64;
        *h.add(1) = cap as i64;
        *h.add(2) = data;
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

extern "C" fn host_array_map(arr: i64, closure: i64) -> i64 {
    if arr == 0 || closure == 0 {
        return build_array(&[]);
    }
    let (len, _cap, data) = unsafe { array_header(arr) };
    let mut out = Vec::with_capacity(len as usize);
    for i in 0..len {
        let cell = unsafe { *((data + i * 8) as *const i64) };
        let v = unsafe { call_closure_1(closure, cell) };
        out.push(v);
    }
    build_array(&out)
}

extern "C" fn host_array_filter(arr: i64, closure: i64) -> i64 {
    if arr == 0 || closure == 0 {
        return build_array(&[]);
    }
    let (len, _cap, data) = unsafe { array_header(arr) };
    let mut out = Vec::new();
    for i in 0..len {
        let cell = unsafe { *((data + i * 8) as *const i64) };
        let keep = unsafe { call_closure_1(closure, cell) };
        if keep != 0 {
            out.push(cell);
        }
    }
    build_array(&out)
}

extern "C" fn host_array_slice(arr: i64, start: i64, end: i64) -> i64 {
    if arr == 0 {
        return build_array(&[]);
    }
    let (len, _cap, data) = unsafe { array_header(arr) };
    let lo = start.max(0).min(len) as usize;
    let hi = end.max(0).min(len) as usize;
    let lo = lo.min(hi);
    let mut out: Vec<i64> = Vec::with_capacity(hi - lo);
    for i in lo..hi {
        let cell = unsafe { *((data + (i as i64) * 8) as *const i64) };
        out.push(cell);
    }
    build_array(&out)
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
    build_array(&parts)
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
        let v = *((data + (len - 1) * 8) as *const i64);
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

extern "C" fn host_identity(p: i64) -> i64 { p }
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
                let bytes = cstr_bytes(raw);
                let s = String::from_utf8_lossy(bytes).into_owned();
                elems.push(leak_cstring(s));
                p = p.add(1);
            }
        }
    }
    let n = elems.len() as i64;
    let header = host_mir_alloc(40);
    let data = host_mir_alloc(n.max(1) * 8);
    unsafe {
        let h = header as *mut i64;
        *h = n;
        *h.add(1) = n;
        *h.add(2) = data;
        *h.add(3) = 1;
        *h.add(4) = 0; // elem_kind_tag: string is non-Object so no cascade
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

extern "C" fn host_os_errno() -> i32 {
    // Best-effort errno: read Rust's libc `errno`.
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

extern "C" fn host_os_set_errno(_code: i32) {
    // No portable Rust API to set errno; no-op for now.
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
    if mm.inner.remove(&mk).is_some() { 1 } else { 0 }
}

extern "C" fn host_map_keys(map: i64) -> i64 {
    if map == 0 {
        return build_array(&[]);
    }
    let mm = unsafe { &*(map as *const ManagedMap) };
    let v: Vec<i64> = mm
        .inner
        .keys()
        .map(|k| {
            // Prefer the original C-string ptr the user inserted, so
            // `keys().includes(orig)` succeeds with raw-ptr equality.
            mm.str_key_origs
                .get(k)
                .copied()
                .unwrap_or_else(|| map_key_to_raw(k))
        })
        .collect();
    build_array(&v)
}

extern "C" fn host_map_values(map: i64) -> i64 {
    if map == 0 {
        return build_array(&[]);
    }
    let mm = unsafe { &*(map as *const ManagedMap) };
    let v: Vec<i64> = mm.inner.values().copied().collect();
    build_array(&v)
}

fn clif_signature_for(
    module: &JITModule,
    f: &MirFunction,
) -> Result<Signature, CompileError> {
    let mut sig = module.make_signature();
    for p in f.params.iter() {
        if let Some(ct) = mir_to_clif(&p.ty) {
            sig.params.push(AbiParam::new(ct));
        } else {
            return Err(CompileError::Unsupported("unit / void params"));
        }
    }
    // Hidden trailing env-pointer param. Direct callers pass 0;
    // indirect (closure) callers pass the closure block pointer.
    sig.params.push(AbiParam::new(types::I64));
    if !matches!(f.ret, MirTy::Unit) {
        let ret = mir_to_clif(&f.ret)
            .ok_or(CompileError::Unsupported("unit return through ABI"))?;
        sig.returns.push(AbiParam::new(ret));
    }
    Ok(sig)
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
                let v = fb.ins().symbol_value(types::I64, gv);
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
            let v = lower_cast(fb, *kind, sv, dst_ty)?;
            vmap.insert(*dst, v);
        }
        Inst::Call { dst, callee, args } => {
            // `console.log(...)` — special-cased variadic. Each
            // argument prints with a per-type host helper, separated
            // by spaces and terminated by a newline.
            if let FuncRef::Builtin(sym) = callee {
                if sym.as_str() == "console_log" {
                    for (i, a) in args.iter().enumerate() {
                        if i > 0 {
                            let r = module.declare_func_in_func(print_ids.space, fb.func);
                            fb.ins().call(r, &[]);
                        }
                        let aty = func.ty_of(*a).clone();
                        let av = vmap[a];
                        emit_print_value(fb, module, print_ids, print_lits, &aty, av);
                    }
                    let r = module.declare_func_in_func(print_ids.newline, fb.func);
                    fb.ins().call(r, &[]);
                    if let Some(d) = dst {
                        // console.log returns Unit — produce a sentinel
                        // for any (unlikely) consumer.
                        let sentinel = fb.ins().iconst(types::I8, 0);
                        vmap.insert(*d, sentinel);
                    }
                    return Ok(());
                }
            }
            let mut arg_vs: Vec<Value> = args
                .iter()
                .map(|a| {
                    *vmap.get(a).unwrap_or_else(|| {
                        panic!(
                            "missing vmap entry for arg {:?} in call to {:?}",
                            a, callee
                        )
                    })
                })
                .collect();
            let (cid, is_builtin) = match callee {
                FuncRef::Local(id) => (
                    *fn_ids.get(id).ok_or_else(|| {
                        CompileError::Other(format!("missing fn id #{}", id.0))
                    })?,
                    false,
                ),
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
            // bitcast to i64).
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
                            *av = fb.ins().uextend(types::I64, *av);
                        }
                    }
                }
            }
            let local_ref = module.declare_func_in_func(cid, fb.func);
            let inst_ref = fb.ins().call(local_ref, &arg_vs);
            if let Some(d) = dst {
                let results = fb.inst_results(inst_ref);
                if let Some(&v) = results.first() {
                    // Coerce the result to match dst's MIR type. Some
                    // host builtins (e.g. `__str_includes`) return i64
                    // even when the MIR sees the result as `Bool` (i8).
                    let dst_ty = func.ty_of(*d).clone();
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
                MirTy::Object(_) => {
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.release_obj, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Fn(_) => {
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.release_closure, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Array { .. } => {
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
                _ => {}
            }
        }
        Inst::Retain { value } => {
            let aty = func.ty_of(*value).clone();
            match &aty {
                MirTy::Object(_) => {
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.retain_obj, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Fn(_) => {
                    let av = vmap[value];
                    let r = module.declare_func_in_func(panic_aux.retain_closure, fb.func);
                    fb.ins().call(r, &[av]);
                }
                MirTy::Array { .. } => {
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
            let v = emit_is_subclass(fb, cid, *class, prog);
            vmap.insert(*dst, v);
        }
        Inst::DowncastOrNone { dst, value, class } => {
            // `value as? Class` → some(value) if dynamic class is
            // a subtype of `class`, else none. Optional<Object> is
            // boxed: we emit NewOptional on the some-branch, 0 on the
            // none-branch, and merge through a block-arg.
            let p = vmap[value];
            let cid = fb.ins().load(types::I64, MemFlags::trusted(), p, 0);
            let cond = emit_is_subclass(fb, cid, *class, prog);

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
            fb.def_var(var, vmap[value]);
        }
        Inst::UseLocal { dst, local } => {
            let var = locals[local.0 as usize];
            let v = fb.use_var(var);
            vmap.insert(*dst, v);
        }
        Inst::NewObject { dst, class, init_args, init } => {
            let layout = &prog.classes[class.0 as usize];
            let n_fields = layout.fields.len() as i64;
            // Layout: [class_id: i64 | field0 | field1 | ...] — one
            // i64 cell per field, 8-byte header for RTTI.
            let size = fb.ins().iconst(types::I64, OBJECT_HEADER_BYTES as i64 + n_fields * 8);
            let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
            let alloc_call = fb.ins().call(alloc_ref, &[size]);
            let ptr = fb.inst_results(alloc_call)[0];
            // Tag with class id so RTTI can recover the dynamic class,
            // and refcount = 1 so Release can fire deinit exactly
            // once when the last owner drops.
            let cid_v = fb.ins().iconst(types::I64, class.0 as i64);
            fb.ins().store(MemFlags::trusted(), cid_v, ptr, 0);
            let one = fb.ins().iconst(types::I64, 1);
            fb.ins().store(MemFlags::trusted(), one, ptr, 8);

            // If a user init exists (FuncId != u32::MAX), call it.
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
            // Layout: 5-i64 header [length | capacity | data_ptr | rc | elem_kind_tag]
            // followed by a separately-allocated `i64×capacity` buffer.
            // elem_kind_tag: 0 = no cascade needed, 1 = elements are
            // Object pointers (deinit them on rc→0).
            let n = items.len() as i64;
            let header_bytes = fb.ins().iconst(types::I64, 40);
            let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
            let call = fb.ins().call(alloc_ref, &[header_bytes]);
            let ptr = fb.inst_results(call)[0];
            let data_bytes = fb.ins().iconst(types::I64, n.max(1) * 8);
            let dcall = fb.ins().call(alloc_ref, &[data_bytes]);
            let data_ptr = fb.inst_results(dcall)[0];

            let len_v = fb.ins().iconst(types::I64, n);
            fb.ins().store(MemFlags::trusted(), len_v, ptr, 0);
            fb.ins().store(MemFlags::trusted(), len_v, ptr, 8);
            fb.ins().store(MemFlags::trusted(), data_ptr, ptr, 16);
            let one = fb.ins().iconst(types::I64, 1);
            fb.ins().store(MemFlags::trusted(), one, ptr, 24);
            let tag = if matches!(elem, MirTy::Object(_)) { 1 } else { 0 };
            let tag_v = fb.ins().iconst(types::I64, tag);
            fb.ins().store(MemFlags::trusted(), tag_v, ptr, 32);
            for (i, it) in items.iter().enumerate() {
                let v_ext = extend_to_i64(fb, vmap[it]);
                fb.ins().store(MemFlags::trusted(), v_ext, data_ptr, (i as i32) * 8);
            }
            vmap.insert(*dst, ptr);
        }
        Inst::NewArrayEmpty { dst, elem, fixed_len } => {
            let n = fixed_len.unwrap_or(0) as i64;
            let header_bytes = fb.ins().iconst(types::I64, 40);
            let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
            let call = fb.ins().call(alloc_ref, &[header_bytes]);
            let ptr = fb.inst_results(call)[0];
            let cap = n.max(4);
            let data_bytes = fb.ins().iconst(types::I64, cap * 8);
            let dcall = fb.ins().call(alloc_ref, &[data_bytes]);
            let data_ptr = fb.inst_results(dcall)[0];
            let len_v = fb.ins().iconst(types::I64, n);
            let cap_v = fb.ins().iconst(types::I64, cap);
            fb.ins().store(MemFlags::trusted(), len_v, ptr, 0);
            fb.ins().store(MemFlags::trusted(), cap_v, ptr, 8);
            fb.ins().store(MemFlags::trusted(), data_ptr, ptr, 16);
            let one = fb.ins().iconst(types::I64, 1);
            fb.ins().store(MemFlags::trusted(), one, ptr, 24);
            let tag = if matches!(elem, MirTy::Object(_)) { 1 } else { 0 };
            let tag_v = fb.ins().iconst(types::I64, tag);
            fb.ins().store(MemFlags::trusted(), tag_v, ptr, 32);
            vmap.insert(*dst, ptr);
        }
        Inst::ArrayLen { dst, arr } => {
            let p = vmap[arr];
            let v = fb.ins().load(types::I64, MemFlags::trusted(), p, 0);
            vmap.insert(*dst, v);
        }
        Inst::ArrayLoad { dst, arr, idx } => {
            let p = vmap[arr];
            let i = vmap[idx];
            let len = fb.ins().load(types::I64, MemFlags::trusted(), p, 0);
            let oob_lo = fb.ins().icmp_imm(IntCC::SignedLessThan, i, 0);
            let oob_hi = fb.ins().icmp(IntCC::SignedGreaterThanOrEqual, i, len);
            let oob = fb.ins().bor(oob_lo, oob_hi);
            emit_panic_if(fb, module, panic_aux.fn_id, panic_aux.msg_oob, oob);
            let data_ptr = fb.ins().load(types::I64, MemFlags::trusted(), p, 16);
            let stride = fb.ins().iconst(types::I64, 8);
            let off = fb.ins().imul(i, stride);
            let addr = fb.ins().iadd(data_ptr, off);
            let dst_ty_mir = func.ty_of(*dst);
            let raw = fb.ins().load(types::I64, MemFlags::trusted(), addr, 0);
            let v = reduce_from_i64(fb, dst_ty_mir, raw);
            vmap.insert(*dst, v);
        }
        Inst::ArrayStore { arr, idx, value } => {
            let p = vmap[arr];
            let i = vmap[idx];
            let len = fb.ins().load(types::I64, MemFlags::trusted(), p, 0);
            let oob_lo = fb.ins().icmp_imm(IntCC::SignedLessThan, i, 0);
            let oob_hi = fb.ins().icmp(IntCC::SignedGreaterThanOrEqual, i, len);
            let oob = fb.ins().bor(oob_lo, oob_hi);
            emit_panic_if(fb, module, panic_aux.fn_id, panic_aux.msg_oob, oob);
            let data_ptr = fb.ins().load(types::I64, MemFlags::trusted(), p, 16);
            let stride = fb.ins().iconst(types::I64, 8);
            let off = fb.ins().imul(i, stride);
            let addr = fb.ins().iadd(data_ptr, off);
            let v_ext = extend_to_i64(fb, vmap[value]);
            fb.ins().store(MemFlags::trusted(), v_ext, addr, 0);
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
            // [tag: i64 | payload...]
            let bytes = fb.ins().iconst(types::I64, (1 + n_payload) * 8);
            let alloc_ref = module.declare_func_in_func(alloc_id, fb.func);
            let call = fb.ins().call(alloc_ref, &[bytes]);
            let ptr = fb.inst_results(call)[0];
            let disc = fb.ins().iconst(types::I64, v.discriminant);
            fb.ins().store(MemFlags::trusted(), disc, ptr, 0);
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
            vmap.insert(*dst, v);
        }
        Inst::NewTuple { dst, items } => {
            // Heterogeneous fixed-arity product. Hidden 16-byte
            // header lives BEFORE the user-facing pointer:
            //   base + 0  = rc
            //   base + 8  = kind_mask  (bit i = 1 ⇒ element i is Object)
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
            // kind_mask
            let dst_ty = func.ty_of(*dst).clone();
            let mut mask: i64 = 0;
            if let MirTy::Tuple(elems) = &dst_ty {
                for (i, ety) in elems.iter().enumerate() {
                    if matches!(ety, MirTy::Object(_)) && i < 64 {
                        mask |= 1i64 << i;
                    }
                }
            }
            let mask_v = fb.ins().iconst(types::I64, mask);
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
            // Array convention: 1 = inner is Object (cascade).
            let dst_ty = func.ty_of(*dst).clone();
            let tag = if let MirTy::Optional(inner) = &dst_ty {
                if matches!(**inner, MirTy::Object(_)) { 1 } else { 0 }
            } else {
                0
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
            let off = OBJECT_HEADER_BYTES + (field.0 as i32) * 8;
            let dst_ty_mir = func.ty_of(*dst).clone();
            let raw = fb.ins().load(types::I64, MemFlags::trusted(), obj_v, off);
            let v = reduce_from_i64(fb, &dst_ty_mir, raw);
            vmap.insert(*dst, v);
        }
        Inst::StoreField { obj, field, value } => {
            let obj_v = vmap[obj];
            let off = OBJECT_HEADER_BYTES + (field.0 as i32) * 8;
            let store_v = extend_to_i64(fb, vmap[value]);
            fb.ins().store(MemFlags::trusted(), store_v, obj_v, off);
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
    let addr = fb.ins().symbol_value(types::I64, gv);
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
            emit_print_value(fb, module, print_ids, print_lits, inner, inner_v);
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
                emit_print_value(fb, module, print_ids, print_lits, ity, elem_v);
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
            emit_print_value(fb, module, print_ids, print_lits, elem, elem_v);
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
            let id_v = fb.ins().iconst(types::I64, eid.0 as i64);
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
    let addr = fb.ins().symbol_value(types::I64, gv);
    let fr = module.declare_func_in_func(panic_fn, fb.func);
    fb.ins().call(fr, &[addr]);
    fb.ins().trap(TrapCode::user(1).unwrap());
    fb.switch_to_block(cont_block);
    fb.seal_block(cont_block);
}

fn lower_binop(fb: &mut ClifFnBuilder, op: BinOp, lhs: Value, rhs: Value) -> Value {
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
) -> Result<Value, CompileError> {
    use ilang_mir::CastKind;
    let dst_ct = mir_to_clif(dst_ty).ok_or(CompileError::Unsupported("cast to non-clif type"))?;
    Ok(match kind {
        CastKind::IntResize | CastKind::IntSignCross => {
            let src_ty = fb.func.dfg.value_type(src);
            if src_ty.bits() == dst_ct.bits() {
                src
            } else if src_ty.bits() < dst_ct.bits() {
                if matches!(kind, CastKind::IntSignCross) {
                    fb.ins().uextend(dst_ct, src)
                } else {
                    fb.ins().sextend(dst_ct, src)
                }
            } else {
                fb.ins().ireduce(dst_ct, src)
            }
        }
        CastKind::IntToFloat => {
            // Default to signed conversion; real code paths thread the
            // source's signedness later.
            fb.ins().fcvt_from_sint(dst_ct, src)
        }
        CastKind::FloatToInt => fb.ins().fcvt_to_sint(dst_ct, src),
        CastKind::FloatResize => {
            let src_ty = fb.func.dfg.value_type(src);
            if src_ty.bits() < dst_ct.bits() {
                fb.ins().fpromote(dst_ct, src)
            } else {
                fb.ins().fdemote(dst_ct, src)
            }
        }
        CastKind::StrongToWeak | CastKind::PtrCast | CastKind::PtrIntCast => {
            // Same-width / same-rep reinterprets — pass the i64 value
            // through unchanged.
            src
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
