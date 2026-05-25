//! JIT entry points: `compile_program` / `compile_with_builtins`.
//!
//! Build a `JITBuilder`, register the canonical `ilang_runtime::__*`
//! symbol table, drive `lower_program_into` to populate the module,
//! finalize, and mirror the program's class / enum / closure /
//! drop-fn metadata into the runtime registries so the cascade and
//! print paths can find it at run-time.

use std::collections::HashMap;

use cranelift_codegen::settings;
use cranelift_jit::{JITBuilder, JITModule};

use ilang_mir::{FuncId, MirTy, Program};

use super::host_misc::{host_optional_missing_stub, lookup_symbol_in_process, process_symbol_exists};
use super::host_os::try_open_lib;
use super::print_kind::{
    kind_tag_of, kind_tag_of_print_kind, print_kind_id, print_kind_id_for_print_kind,
    print_kind_of, PrintKind, KIND_NONE,
};
use super::{
    alloc_global_class_id, alloc_global_enum_id, lower_program_into, walk_mir_ty, BuiltinDecl,
    Compiled, CompileError, LoweringOutputs, OBJECT_HEADER_BYTES,
};

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
    // On Windows, Cranelift's built-in resolver only checks the main
    // executable and ucrtbase.dll. Register a custom lookup that
    // searches all loaded modules so DLLs opened via LoadLibraryA
    // (e.g. SDL2.dll) are visible to the JIT.
    jit_builder.symbol_lookup_fn(Box::new(|name| lookup_symbol_in_process(name)));
    // Always-available allocator. Allocates `size` bytes (zero-init)
    // via Rust's `Vec<u8>` and leaks the pointer. The MIR codegen's
    // ARC step is what eventually frees it; until then it's a small
    // intentional leak that's fine for short-running test programs.
    jit_builder.symbol("$alloc.alloc", ilang_runtime::__mir_alloc as *const u8);
    jit_builder.symbol("$alloc.free", ilang_runtime::__mir_free as *const u8);
    // Map runtime backed by Rust's HashMap<i64, i64> (one box per
    // map). Keys / values flow through as i64 cells (heap pointers
    // share identity when interned).
    jit_builder.symbol("$map.new", ilang_runtime::__map_new as *const u8);
    jit_builder.symbol("$map.get", ilang_runtime::__map_get as *const u8);
    jit_builder.symbol("$map.getOptional", ilang_runtime::__map_get_optional as *const u8);
    jit_builder.symbol("$map.set", ilang_runtime::__map_set as *const u8);
    jit_builder.symbol("$map.has", ilang_runtime::__map_has as *const u8);
    jit_builder.symbol("$map.size", ilang_runtime::__map_size as *const u8);
    jit_builder.symbol("$map.delete", ilang_runtime::__map_delete as *const u8);
    jit_builder.symbol("$map.keys", ilang_runtime::__map_keys as *const u8);
    jit_builder.symbol("$map.values", ilang_runtime::__map_values as *const u8);
    // Promise + thread pool runtime.
    jit_builder.symbol("$promise.resolve", ilang_runtime::__promise_resolve as *const u8);
    jit_builder.symbol("$promise.reject", ilang_runtime::__promise_reject as *const u8);
    jit_builder.symbol("$promise.then", ilang_runtime::__promise_then as *const u8);
    jit_builder.symbol("$promise.catch", ilang_runtime::__promise_catch as *const u8);
    jit_builder.symbol(
        "$promise.withExecutor",
        ilang_runtime::__promise_with_executor as *const u8,
    );
    jit_builder.symbol("$promise.drain", ilang_runtime::__promise_drain as *const u8);
    jit_builder.symbol("$promise.all", ilang_runtime::__promise_all as *const u8);
    jit_builder.symbol("$promise.race", ilang_runtime::__promise_race as *const u8);
    jit_builder.symbol(
        "$promise.settleResolve",
        ilang_runtime::__promise_settle_resolve as *const u8,
    );
    jit_builder.symbol(
        "$promise.settleReject",
        ilang_runtime::__promise_settle_reject as *const u8,
    );
    jit_builder.symbol(
        "$promise.pending",
        ilang_runtime::__promise_pending as *const u8,
    );
    jit_builder.symbol("$promise.retain", ilang_runtime::__retain_promise as *const u8);
    jit_builder.symbol("$promise.release", ilang_runtime::__release_promise as *const u8);
    // ObjC block dispatcher — `new ObjCBlock(closure)` desugars to a
    // call here with a kind selector chosen from the closure's fn
    // signature.
    #[cfg(target_os = "macos")]
    jit_builder.symbol(
        "$objc.make_block",
        ilang_runtime::make_objc_block as *const u8,
    );
    // ObjCBlock.invoke(args) per-shape entry points — dispatch by
    // arg shape in MIR's `lower_method_call`.
    jit_builder.symbol(
        "$objc.invoke_void_block",
        ilang_runtime::invoke_void_block_via_runtime as *const u8,
    );
    jit_builder.symbol(
        "$objc.invoke_obj_block",
        ilang_runtime::invoke_obj_block_via_runtime as *const u8,
    );
    jit_builder.symbol(
        "$objc.invoke_obj_to_obj_block",
        ilang_runtime::invoke_obj_to_obj_block_via_runtime as *const u8,
    );
    jit_builder.symbol(
        "$objc.invoke_void_bytes_block",
        ilang_runtime::invoke_void_bytes_block_via_runtime as *const u8,
    );
    jit_builder.symbol(
        "$objc.invoke_void_three_obj_block",
        ilang_runtime::invoke_void_three_obj_block_via_runtime as *const u8,
    );
    jit_builder.symbol(
        "$objc.invoke_void_bool_block",
        ilang_runtime::invoke_void_bool_block_via_runtime as *const u8,
    );
    // Default string builtins. Returns are NUL-terminated `*const u8`
    // pointers to leaked Rust-side allocations. Acceptable until the
    // ARC-backed StringRc runtime arrives.
    jit_builder.symbol("$string.length", ilang_runtime::__str_length as *const u8);
    jit_builder.symbol("$string.concat", ilang_runtime::__str_concat as *const u8);
    jit_builder.symbol(
        "$string.concatInplace",
        ilang_runtime::__str_concat_inplace as *const u8,
    );
    jit_builder.symbol("$string.eq", ilang_runtime::__str_eq as *const u8);
    jit_builder.symbol("$string.fromInt", ilang_runtime::__int_to_string as *const u8);
    jit_builder.symbol("$string.fromBool", ilang_runtime::__bool_to_string as *const u8);
    jit_builder.symbol("$string.toUpper", ilang_runtime::__str_to_upper as *const u8);
    jit_builder.symbol("$string.toLower", ilang_runtime::__str_to_lower as *const u8);
    jit_builder.symbol("$string.trim", ilang_runtime::__str_trim as *const u8);
    jit_builder.symbol("$string.includes", ilang_runtime::__str_includes as *const u8);
    jit_builder.symbol("$string.startsWith", ilang_runtime::__str_starts_with as *const u8);
    jit_builder.symbol("$string.endsWith", ilang_runtime::__str_ends_with as *const u8);
    jit_builder.symbol("$string.charAt", ilang_runtime::__str_char_at as *const u8);
    jit_builder.symbol("$string.slice", ilang_runtime::__str_slice as *const u8);
    jit_builder.symbol("$string.replace", ilang_runtime::__str_replace as *const u8);
    jit_builder.symbol("$array.indexOf", ilang_runtime::__array_index_of as *const u8);
    jit_builder.symbol("$array.includes", ilang_runtime::__array_includes as *const u8);
    jit_builder.symbol("$array.push", ilang_runtime::__array_push as *const u8);
    jit_builder.symbol("$array.pop", ilang_runtime::__array_pop as *const u8);
    jit_builder.symbol("$array.remove", ilang_runtime::__array_remove as *const u8);
    jit_builder.symbol("$array.removeAt", ilang_runtime::__array_remove_at as *const u8);
    jit_builder.symbol("$array.find", ilang_runtime::__array_find as *const u8);
    jit_builder.symbol("$array.findIndex", ilang_runtime::__array_find_index as *const u8);
    jit_builder.symbol("$array.every", ilang_runtime::__array_every as *const u8);
    jit_builder.symbol("$array.some", ilang_runtime::__array_some as *const u8);
    jit_builder.symbol("$array.concat", ilang_runtime::__array_concat as *const u8);
    jit_builder.symbol("$array.reverse", ilang_runtime::__array_reverse as *const u8);
    jit_builder.symbol("$array.join", ilang_runtime::__array_join as *const u8);
    jit_builder.symbol("$array.shift", ilang_runtime::__array_shift as *const u8);
    jit_builder.symbol("$array.unshift", ilang_runtime::__array_unshift as *const u8);
    jit_builder.symbol("$array.fill", ilang_runtime::__array_fill as *const u8);
    jit_builder.symbol("$array.sort", ilang_runtime::__array_sort as *const u8);
    jit_builder.symbol("$array.fixedToDyn", ilang_runtime::__fixed_to_dyn as *const u8);
    jit_builder.symbol("$enum.box", ilang_runtime::__enum_box as *const u8);
    jit_builder.symbol("$ffi.arrayFromCArray", ilang_runtime::__c_array_to_array as *const u8);
    jit_builder.symbol("$repl.loadSlot", ilang_runtime::__repl_load_slot as *const u8);
    jit_builder.symbol("$repl.storeSlot", ilang_runtime::__repl_store_slot as *const u8);
    // Raw-memory FFI marshalling: `readT(p, off): T` / `writeT(p,
    // off, v)`. The `read*` family folds the loaded primitive to
    // i64 (or f32/f64) for the cross-FFI return; callers reinterpret
    // via the slot-typing handled in `lower_call`.
    jit_builder.symbol("$ffi.readI8", ilang_runtime::__read_i8 as *const u8);
    jit_builder.symbol("$ffi.readI16", ilang_runtime::__read_i16 as *const u8);
    jit_builder.symbol("$ffi.readI32", ilang_runtime::__read_i32 as *const u8);
    jit_builder.symbol("$ffi.readI64", ilang_runtime::__read_i64 as *const u8);
    jit_builder.symbol("$ffi.readU8", ilang_runtime::__read_u8 as *const u8);
    jit_builder.symbol("$ffi.readU16", ilang_runtime::__read_u16 as *const u8);
    jit_builder.symbol("$ffi.readU32", ilang_runtime::__read_u32 as *const u8);
    jit_builder.symbol("$ffi.readU64", ilang_runtime::__read_u64 as *const u8);
    jit_builder.symbol("$ffi.readF32", ilang_runtime::__read_f32 as *const u8);
    jit_builder.symbol("$ffi.readF64", ilang_runtime::__read_f64 as *const u8);
    jit_builder.symbol("$ffi.writeI8", ilang_runtime::__write_i8 as *const u8);
    jit_builder.symbol("$ffi.writeI16", ilang_runtime::__write_i16 as *const u8);
    jit_builder.symbol("$ffi.writeI32", ilang_runtime::__write_i32 as *const u8);
    jit_builder.symbol("$ffi.writeI64", ilang_runtime::__write_i64 as *const u8);
    jit_builder.symbol("$ffi.writeU8", ilang_runtime::__write_u8 as *const u8);
    jit_builder.symbol("$ffi.writeU16", ilang_runtime::__write_u16 as *const u8);
    jit_builder.symbol("$ffi.writeU32", ilang_runtime::__write_u32 as *const u8);
    jit_builder.symbol("$ffi.writeU64", ilang_runtime::__write_u64 as *const u8);
    jit_builder.symbol("$ffi.writeF32", ilang_runtime::__write_f32 as *const u8);
    jit_builder.symbol("$ffi.writeF64", ilang_runtime::__write_f64 as *const u8);
    jit_builder.symbol("$array.map", ilang_runtime::__array_map as *const u8);
    jit_builder.symbol("$array.filter", ilang_runtime::__array_filter as *const u8);
    jit_builder.symbol("$array.forEach", ilang_runtime::__array_for_each as *const u8);
    jit_builder.symbol("$array.slice", ilang_runtime::__array_slice as *const u8);
    jit_builder.symbol("$string.split", ilang_runtime::__str_split as *const u8);
    jit_builder.symbol("$class.virtDispatch", ilang_runtime::__virt_dispatch as *const u8);
    jit_builder.symbol("$class.dropDispatch", ilang_runtime::__drop_dispatch as *const u8);
    jit_builder.symbol("$print.object", ilang_runtime::__print_object as *const u8);
    jit_builder.symbol("$print.struct", ilang_runtime::__print_struct as *const u8);
    jit_builder.symbol("$class.name", ilang_runtime::__class_name as *const u8);
    jit_builder.symbol("$print.weak", ilang_runtime::__print_weak as *const u8);
    jit_builder.symbol("$print.enum", ilang_runtime::__print_enum as *const u8);
    jit_builder.symbol("$print.fn", ilang_runtime::__print_fn as *const u8);
    jit_builder.symbol("$class.releaseObject", ilang_runtime::__release_object as *const u8);
    jit_builder.symbol("$class.retainObject", ilang_runtime::__retain_object as *const u8);
    jit_builder.symbol("$closure.release", ilang_runtime::__release_closure as *const u8);
    jit_builder.symbol("$closure.retain", ilang_runtime::__retain_closure as *const u8);
    jit_builder.symbol("$array.release", ilang_runtime::__release_array as *const u8);
    jit_builder.symbol("$array.retain", ilang_runtime::__retain_array as *const u8);
    jit_builder.symbol("$optional.release", ilang_runtime::__release_optional as *const u8);
    jit_builder.symbol("$optional.retain", ilang_runtime::__retain_optional as *const u8);
    jit_builder.symbol("$tuple.release", ilang_runtime::__release_tuple as *const u8);
    jit_builder.symbol("$tuple.retain", ilang_runtime::__retain_tuple as *const u8);
    jit_builder.symbol("$map.release", ilang_runtime::__release_map as *const u8);
    jit_builder.symbol("$map.retain", ilang_runtime::__retain_map as *const u8);
    jit_builder.symbol("$string.release", ilang_runtime::__release_string as *const u8);
    jit_builder.symbol("$string.retain", ilang_runtime::__retain_string as *const u8);
    // Always-on memory-tracking helpers exposed through `test.liveAlloc*`
    // / `test.liveStringCount`. Used by the leak-detection fixtures
    // under tests/programs/.
    jit_builder.symbol("$test.liveAllocBytes", ilang_runtime::test_live_alloc_bytes as *const u8);
    jit_builder.symbol("$test.liveAllocCount", ilang_runtime::test_live_alloc_count as *const u8);
    jit_builder.symbol("$test.liveStringCount", ilang_runtime::test_live_string_count as *const u8);
    jit_builder.symbol(
        "test.mallocBytesInUse",
        ilang_runtime::test_malloc_bytes_in_use as *const u8,
    );
    jit_builder.symbol("$enum.alloc", ilang_runtime::__enum_alloc as *const u8);
    jit_builder.symbol("$enum.release", ilang_runtime::__release_enum as *const u8);
    jit_builder.symbol("$enum.retain", ilang_runtime::__retain_enum as *const u8);
    jit_builder.symbol("$enum.unitGet", ilang_runtime::__enum_unit_get as *const u8);
    jit_builder.symbol(
        "$enum.unitGetChecked",
        ilang_runtime::__enum_unit_get_checked as *const u8,
    );
    jit_builder.symbol("$enum.discStr", ilang_runtime::__enum_disc_str as *const u8);
    jit_builder.symbol("$map.setValueKind", ilang_runtime::__map_set_value_kind as *const u8);
    jit_builder.symbol("$map.setPrintKinds", ilang_runtime::__map_set_print_kinds as *const u8);
    jit_builder.symbol("$print.map", ilang_runtime::__print_map as *const u8);
    // FFI marshalling helpers — registered both with their bare names
    // (used inside `@extern(C)` blocks) and qualified names. Strings
    // are NUL-terminated `*const u8` already, so most "C-string"
    // helpers are identity at the bit level.
    jit_builder.symbol("$array.dataPtr", ilang_runtime::__array_data_ptr as *const u8);
    jit_builder.symbol("$ffi.cstrFromString", ilang_runtime::cstr_from_string as *const u8);
    jit_builder.symbol("$ffi.stringFromCstr", ilang_runtime::string_from_cstr as *const u8);
    jit_builder.symbol("$ffi.cstrArrayToStrings", ilang_runtime::cstr_array_to_strings as *const u8);
    jit_builder.symbol("$ffi.freeCstr", ilang_runtime::free_cstr as *const u8);
    jit_builder.symbol("$ffi.errnoCheck", ilang_runtime::errno_check_i32 as *const u8);
    jit_builder.symbol("$ffi.errnoCheckI64", ilang_runtime::errno_check_i64 as *const u8);
    jit_builder.symbol("$ffi.bytesFromBuffer", ilang_runtime::bytes_from_buffer as *const u8);
    jit_builder.symbol("$os.errno", ilang_runtime::os_errno as *const u8);
    jit_builder.symbol("$os.setErrno", ilang_runtime::os_set_errno as *const u8);
    jit_builder.symbol("$os.libLoaded", ilang_runtime::os_lib_loaded as *const u8);
    jit_builder.symbol("$os.libLoadError", ilang_runtime::os_lib_load_error as *const u8);
    jit_builder.symbol("$os.platform", ilang_runtime::os_platform as *const u8);
    jit_builder.symbol(
        "$ilang.objcImpLookup",
        ilang_runtime::__ilang_objc_imp_lookup as *const u8,
    );
    jit_builder.symbol(
        "$objc.make_void_block",
        ilang_runtime::make_void_block as *const u8,
    );
    jit_builder.symbol(
        "$objc.invoke_void_block",
        ilang_runtime::invoke_void_block_via_runtime as *const u8,
    );
    jit_builder.symbol(
        "$objc.make_obj_block",
        ilang_runtime::make_obj_block as *const u8,
    );
    jit_builder.symbol(
        "$objc.invoke_obj_block",
        ilang_runtime::invoke_obj_block_via_runtime as *const u8,
    );
    jit_builder.symbol(
        "$objc.make_obj_to_obj_block",
        ilang_runtime::make_obj_to_obj_block as *const u8,
    );
    jit_builder.symbol(
        "$objc.invoke_obj_to_obj_block",
        ilang_runtime::invoke_obj_to_obj_block_via_runtime as *const u8,
    );
    jit_builder.symbol(
        "$objc.make_void_bytes_block",
        ilang_runtime::make_void_bytes_block as *const u8,
    );
    jit_builder.symbol(
        "$objc.make_void_three_obj_block",
        ilang_runtime::make_void_three_obj_block as *const u8,
    );
    jit_builder.symbol(
        "$objc.err_slot_ptr",
        ilang_runtime::objc_err_slot_ptr as *const u8,
    );
    jit_builder.symbol(
        "$objc.take_err",
        ilang_runtime::objc_take_err as *const u8,
    );
    // fs.* — `libs/std/fs.il`'s `@extern(C)` block.
    jit_builder.symbol("$fs.hasError", ilang_runtime::fs::fs_has_error as *const u8);
    jit_builder.symbol("$fs.errorCode", ilang_runtime::fs::fs_error_code as *const u8);
    jit_builder.symbol("$fs.errorMessage", ilang_runtime::fs::fs_error_message as *const u8);
    jit_builder.symbol("$fs.readFile", ilang_runtime::fs::fs_read_file as *const u8);
    jit_builder.symbol("$fs.readFileBytes", ilang_runtime::fs::fs_read_file_bytes as *const u8);
    jit_builder.symbol("$fs.writeFile", ilang_runtime::fs::fs_write_file as *const u8);
    jit_builder.symbol("$fs.writeFileBytes", ilang_runtime::fs::fs_write_file_bytes as *const u8);
    jit_builder.symbol("$fs.appendFile", ilang_runtime::fs::fs_append_file as *const u8);
    jit_builder.symbol("$fs.exists", ilang_runtime::fs::fs_exists as *const u8);
    jit_builder.symbol("$fs.isFile", ilang_runtime::fs::fs_is_file as *const u8);
    jit_builder.symbol("$fs.isDir", ilang_runtime::fs::fs_is_dir as *const u8);
    jit_builder.symbol("$fs.mkdir", ilang_runtime::fs::fs_mkdir as *const u8);
    jit_builder.symbol("$fs.rm", ilang_runtime::fs::fs_rm as *const u8);
    jit_builder.symbol("$fs.rmdir", ilang_runtime::fs::fs_rmdir as *const u8);
    jit_builder.symbol("$fs.rename", ilang_runtime::fs::fs_rename as *const u8);
    jit_builder.symbol("$fs.readDir", ilang_runtime::fs::fs_read_dir as *const u8);
    jit_builder.symbol("$fs.size", ilang_runtime::fs::fs_size as *const u8);
    jit_builder.symbol("$fs.stat", ilang_runtime::fs::fs_stat as *const u8);
    jit_builder.symbol("$fs.lstat", ilang_runtime::fs::fs_lstat as *const u8);
    jit_builder.symbol("$fs.access", ilang_runtime::fs::fs_access as *const u8);
    jit_builder.symbol("$fs.copyFile", ilang_runtime::fs::fs_copy_file as *const u8);
    jit_builder.symbol("$fs.cp", ilang_runtime::fs::fs_cp as *const u8);
    jit_builder.symbol("$fs.realpath", ilang_runtime::fs::fs_realpath as *const u8);
    jit_builder.symbol("$fs.chmod", ilang_runtime::fs::fs_chmod as *const u8);
    jit_builder.symbol("$fs.truncate", ilang_runtime::fs::fs_truncate as *const u8);
    jit_builder.symbol("$fs.utimes", ilang_runtime::fs::fs_utimes as *const u8);
    jit_builder.symbol("$fs.symlink", ilang_runtime::fs::fs_symlink as *const u8);
    jit_builder.symbol("$fs.readlink", ilang_runtime::fs::fs_readlink as *const u8);
    jit_builder.symbol("$fs.link", ilang_runtime::fs::fs_link as *const u8);
    jit_builder.symbol("$fs.mkdtemp", ilang_runtime::fs::fs_mkdtemp as *const u8);
    // time.* — `libs/std/time.il`'s `@extern(C)` block.
    jit_builder.symbol("$time.now_ms", ilang_runtime::time::time_now_ms as *const u8);
    jit_builder.symbol("$time.now_ns", ilang_runtime::time::time_now_ns as *const u8);
    jit_builder.symbol("$time.monotonic_ns", ilang_runtime::time::time_monotonic_ns as *const u8);
    jit_builder.symbol("$time.sleep_ms", ilang_runtime::time::time_sleep_ms as *const u8);
    jit_builder.symbol("$time.break_down_utc", ilang_runtime::time::time_break_down_utc as *const u8);
    jit_builder.symbol("$time.break_down_local", ilang_runtime::time::time_break_down_local as *const u8);
    jit_builder.symbol("$time.compose", ilang_runtime::time::time_compose as *const u8);
    jit_builder.symbol("$time.parse_iso", ilang_runtime::time::time_parse_iso as *const u8);
    jit_builder.symbol("$time.to_iso", ilang_runtime::time::time_to_iso as *const u8);
    jit_builder.symbol("$time.format", ilang_runtime::time::time_format as *const u8);
    // regex.* — `libs/std/regex.il` binds these via `@intrinsic("regex.X")`
    // declarations. The runtime exports each backing fn under the same
    // `regex.X` symbol name.
    jit_builder.symbol("$regex.compile", ilang_runtime::regex::__regex_compile as *const u8);
    jit_builder.symbol("$regex.destroy", ilang_runtime::regex::__regex_destroy as *const u8);
    jit_builder.symbol("$regex.test", ilang_runtime::regex::__regex_test as *const u8);
    jit_builder.symbol("$regex.has_match", ilang_runtime::regex::__regex_has_match as *const u8);
    jit_builder.symbol("$regex.first_match", ilang_runtime::regex::__regex_first_match as *const u8);
    jit_builder.symbol("$regex.replace_all", ilang_runtime::regex::__regex_replace_all as *const u8);
    jit_builder.symbol("$regex.find_all", ilang_runtime::regex::__regex_find_all as *const u8);
    jit_builder.symbol("$regex.split", ilang_runtime::regex::__regex_split as *const u8);
    // Built-in `test.*` runtime — fixture programs use these to
    // self-check. Failures abort the process with exit code 2.
    // Reuse the legacy JIT's full test-extern symbol set (callbacks,
    // by-value structs, sret returns, errno helpers, etc), then
    // override the closure-callback shim with our mir-aware one
    // `test.*` symbols (incl. test.countedFree*) live in
    // `ilang-runtime` now; the explicit `jit_builder.symbol(...)`
    // bindings below pick them up.
    jit_builder.symbol("$test.applyI32Cb", ilang_runtime::test_apply_i32_cb as *const u8);
    jit_builder.symbol("$test.expect", ilang_runtime::test_expect as *const u8);
    jit_builder.symbol("$test.expectStr", ilang_runtime::test_expect_str as *const u8);
    jit_builder.symbol("$test.expectBool", ilang_runtime::test_expect_bool as *const u8);
    jit_builder.symbol("$test.expectF64", ilang_runtime::test_expect_f64 as *const u8);
    jit_builder.symbol("$test.expectTrue", ilang_runtime::test_expect_true as *const u8);
    jit_builder.symbol("$test.expectFalse", ilang_runtime::test_expect_false as *const u8);
    jit_builder.symbol("$test.fail", ilang_runtime::test_fail as *const u8);
    jit_builder.symbol("$test.countedFree", ilang_runtime::test_counted_free as *const u8);
    jit_builder.symbol("$test.countedFreeCount", ilang_runtime::test_counted_free_count as *const u8);
    // Built-in `math.*` runtime — wraps `f64::*` Rust intrinsics.
    jit_builder.symbol("$math.sin", ilang_runtime::math_sin as *const u8);
    jit_builder.symbol("$math.cos", ilang_runtime::math_cos as *const u8);
    jit_builder.symbol("$math.tan", ilang_runtime::math_tan as *const u8);
    jit_builder.symbol("$math.asin", ilang_runtime::math_asin as *const u8);
    jit_builder.symbol("$math.acos", ilang_runtime::math_acos as *const u8);
    jit_builder.symbol("$math.atan", ilang_runtime::math_atan as *const u8);
    jit_builder.symbol("$math.atan2", ilang_runtime::math_atan2 as *const u8);
    jit_builder.symbol("$math.sqrt", ilang_runtime::math_sqrt as *const u8);
    jit_builder.symbol("$math.pow", ilang_runtime::math_pow as *const u8);
    jit_builder.symbol("$math.exp", ilang_runtime::math_exp as *const u8);
    jit_builder.symbol("$math.ln", ilang_runtime::math_ln as *const u8);
    jit_builder.symbol("$math.log10", ilang_runtime::math_log10 as *const u8);
    jit_builder.symbol("$math.log2", ilang_runtime::math_log2 as *const u8);
    jit_builder.symbol("$math.floor", ilang_runtime::math_floor as *const u8);
    jit_builder.symbol("$math.ceil", ilang_runtime::math_ceil as *const u8);
    jit_builder.symbol("$math.round", ilang_runtime::math_round as *const u8);
    jit_builder.symbol("$math.abs", ilang_runtime::math_abs as *const u8);
    // `console.log` is variadic at the language surface, so the
    // codegen splits each argument into a per-type print call.
    jit_builder.symbol("$ilang.panic", ilang_runtime::__ilang_panic as *const u8);
    // Print helpers and `__ilang_panic` live in `ilang-runtime` so JIT
    // and AOT share the same `extern "C"` bodies. We feed JIT the
    // pointer; AOT links against the `.a` facet at build time.
    jit_builder.symbol("$print.int", ilang_runtime::__print_int as *const u8);
    jit_builder.symbol("$print.bool", ilang_runtime::__print_bool as *const u8);
    jit_builder.symbol("$print.f64", ilang_runtime::__print_f64 as *const u8);
    jit_builder.symbol("$print.str", ilang_runtime::__print_str as *const u8);
    jit_builder.symbol("$print.space", ilang_runtime::__print_space as *const u8);
    jit_builder.symbol("$print.newline", ilang_runtime::__print_newline as *const u8);
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
    let LoweringOutputs {
        fn_ids,
        extern_fn_ids,
        missing_optional_fn_ids: _,
        extern_alias_fn_ids: _,
    } = lower_program_into(&mut module, prog, builtins, &class_global, &enum_global)?;
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
    // Register `@objc class : Parent` IMP function addresses with
    // the runtime so the parser-generated `register()` body can
    // resolve them via `__ilang_objc_imp_lookup` — JIT-emitted
    // functions aren't reachable through `dlsym(RTLD_DEFAULT)`.
    for (idx, f) in prog.functions.iter().enumerate() {
        let mid = FuncId(idx as u32);
        if extern_fn_ids.contains(&mid) {
            continue;
        }
        let symbol_name = f
            .c_symbol
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or_else(|| f.name.as_str());
        if !symbol_name.starts_with("$objc.imp.") {
            continue;
        }
        if let Some(cl_id) = fn_ids.get(&mid) {
            let addr = module.get_finalized_function(*cl_id) as usize;
            ilang_runtime::__register_objc_imp(symbol_name.to_string(), addr);
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

        // Populate the runtime's per-class cascade table so
        // `__release_object_fields` walks heap-shaped fields at
        // rc = 0. Both backends populate the same `ilang-runtime`
        // registry (AOT mirrors these calls from `__ilang_aot_init`).
        for class in &prog.classes {
            for (i, f) in class.fields.iter().enumerate() {
                let cascade_tag = kind_tag_of(&f.ty, &prog.classes);
                if cascade_tag != KIND_NONE {
                    let off = OBJECT_HEADER_BYTES as i64 + (i as i64) * 8;
                    ilang_runtime::__register_object_field(
                        global_cid(class.id.0) as i64,
                        off,
                        cascade_tag,
                    );
                }
            }
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
    // Populate the enum print registry (`__register_enum_print_name`
    // / variant names / payload kinds) and the per-variant cascade
    // table in `ilang-runtime` so both backends see the same enum
    // metadata. AOT mirrors these registrations from
    // `__ilang_aot_init`.
    for e in &prog.enums {
        let global_id = global_eid(e.id.0);
        let name_ptr = ilang_runtime::leak_cstring(e.name.as_str().to_string());
        ilang_runtime::__register_enum_print_name(global_id as i64, name_ptr);
        let is_str_repr = matches!(e.repr, MirTy::Str);
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
            if is_str_repr {
                if let Some(s) = v.discriminant_str.as_ref() {
                    // Mirror into the runtime registry that
                    // `__enum_disc_str` reads so AOT-built programs
                    // see the same mapping.
                    let sp = ilang_runtime::leak_cstring(s.clone());
                    ilang_runtime::__register_enum_disc_str(
                        global_id as i64,
                        v.discriminant,
                        sp,
                    );
                }
            }
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
            let tag = kind_tag_of(&cap.ty, &prog.classes);
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
            if !name.starts_with("$anon.fn_") && !name.starts_with("$main") {
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
