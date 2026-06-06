//! Runtime-symbol table for the JIT.
//!
//! Carries every `$*` name that the codegen may reference back to its
//! Rust impl in `ilang_runtime`. Kept in a dedicated file so
//! `jit_setup.rs` reads as control flow rather than a 450-line list
//! of name → fn-pointer pairs. AOT links the same Rust crate
//! statically, so it doesn't need this table.

use cranelift_jit::JITBuilder;

/// Register every canonical `$*` runtime symbol with the JIT
/// builder so unresolved references emitted by the codegen find
/// their host implementation.
pub(super) fn register_runtime_symbols(jit_builder: &mut JITBuilder) {
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
    jit_builder.symbol("$map.clear", ilang_runtime::__map_clear as *const u8);
    jit_builder.symbol("$map.entries", ilang_runtime::__map_entries as *const u8);
    jit_builder.symbol("$map.forEach", ilang_runtime::__map_for_each as *const u8);
    // Set runtime.
    jit_builder.symbol("$set.new", ilang_runtime::__set_new as *const u8);
    jit_builder.symbol("$set.newObject", ilang_runtime::__set_new_object as *const u8);
    jit_builder.symbol("$map.newObject", ilang_runtime::__map_new_object as *const u8);
    jit_builder.symbol("$set.add", ilang_runtime::__set_add as *const u8);
    jit_builder.symbol("$set.has", ilang_runtime::__set_has as *const u8);
    jit_builder.symbol("$set.delete", ilang_runtime::__set_delete as *const u8);
    jit_builder.symbol("$set.size", ilang_runtime::__set_size as *const u8);
    jit_builder.symbol("$set.clear", ilang_runtime::__set_clear as *const u8);
    jit_builder.symbol("$set.retain", ilang_runtime::__retain_set as *const u8);
    jit_builder.symbol("$set.release", ilang_runtime::__release_set as *const u8);
    jit_builder.symbol(
        "$set.setElemPrintKind",
        ilang_runtime::__set_set_elem_print_kind as *const u8,
    );
    jit_builder.symbol("$set.addF32", ilang_runtime::__set_add_f32 as *const u8);
    jit_builder.symbol("$set.addF64", ilang_runtime::__set_add_f64 as *const u8);
    jit_builder.symbol("$set.hasF32", ilang_runtime::__set_has_f32 as *const u8);
    jit_builder.symbol("$set.hasF64", ilang_runtime::__set_has_f64 as *const u8);
    jit_builder.symbol("$set.deleteF32", ilang_runtime::__set_delete_f32 as *const u8);
    jit_builder.symbol("$set.deleteF64", ilang_runtime::__set_delete_f64 as *const u8);
    jit_builder.symbol("$set.values", ilang_runtime::__set_values as *const u8);
    jit_builder.symbol("$set.forEach", ilang_runtime::__set_for_each as *const u8);
    jit_builder.symbol("$set.forEachF32", ilang_runtime::__set_for_each_f32 as *const u8);
    jit_builder.symbol("$set.forEachF64", ilang_runtime::__set_for_each_f64 as *const u8);
    jit_builder.symbol("$set.union", ilang_runtime::__set_union as *const u8);
    jit_builder.symbol(
        "$set.intersection",
        ilang_runtime::__set_intersection as *const u8,
    );
    jit_builder.symbol("$set.difference", ilang_runtime::__set_difference as *const u8);
    jit_builder.symbol(
        "$set.isSubsetOf",
        ilang_runtime::__set_is_subset_of as *const u8,
    );
    jit_builder.symbol(
        "$set.isSupersetOf",
        ilang_runtime::__set_is_superset_of as *const u8,
    );
    jit_builder.symbol(
        "$set.isDisjointFrom",
        ilang_runtime::__set_is_disjoint_from as *const u8,
    );
    jit_builder.symbol("$print.set", ilang_runtime::__print_set as *const u8);
    jit_builder.symbol("$fmt.set", ilang_runtime::__fmt_set as *const u8);
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
    jit_builder.symbol(
        "$string.fromF32",
        ilang_runtime::__float_to_string_f32 as *const u8,
    );
    jit_builder.symbol(
        "$string.fromF64",
        ilang_runtime::__float_to_string_f64 as *const u8,
    );
    jit_builder.symbol("$string.toUpper", ilang_runtime::__str_to_upper as *const u8);
    jit_builder.symbol("$string.toLower", ilang_runtime::__str_to_lower as *const u8);
    jit_builder.symbol("$string.trim", ilang_runtime::__str_trim as *const u8);
    jit_builder.symbol("$string.includes", ilang_runtime::__str_includes as *const u8);
    jit_builder.symbol("$string.startsWith", ilang_runtime::__str_starts_with as *const u8);
    jit_builder.symbol("$string.endsWith", ilang_runtime::__str_ends_with as *const u8);
    jit_builder.symbol("$string.charAt", ilang_runtime::__str_char_at as *const u8);
    jit_builder.symbol("$string.slice", ilang_runtime::__str_slice as *const u8);
    jit_builder.symbol("$string.replace", ilang_runtime::__str_replace as *const u8);
    jit_builder.symbol("$string.indexOf", ilang_runtime::__str_index_of as *const u8);
    jit_builder.symbol(
        "$string.lastIndexOf",
        ilang_runtime::__str_last_index_of as *const u8,
    );
    jit_builder.symbol(
        "$string.encodeUtf16",
        ilang_runtime::__str_encode_utf16 as *const u8,
    );
    jit_builder.symbol("$string.hashCode", ilang_runtime::__str_hash_code as *const u8);
    jit_builder.symbol(
        "$string.fromUtf16",
        ilang_runtime::__str_from_utf16 as *const u8,
    );
    // Template-literal `$fmt.*` formatters. Mirror the per-type host
    // dispatch in `fmt_emit::emit_format_value`; each one returns a
    // newly-allocated ilang string.
    jit_builder.symbol("$fmt.int", ilang_runtime::__fmt_int as *const u8);
    jit_builder.symbol("$fmt.bool", ilang_runtime::__fmt_bool as *const u8);
    jit_builder.symbol("$fmt.f64", ilang_runtime::__fmt_f64 as *const u8);
    jit_builder.symbol("$fmt.str", ilang_runtime::__fmt_str as *const u8);
    jit_builder.symbol("$fmt.weak", ilang_runtime::__fmt_weak as *const u8);
    jit_builder.symbol("$fmt.fn", ilang_runtime::__fmt_fn as *const u8);
    jit_builder.symbol("$fmt.object", ilang_runtime::__fmt_object as *const u8);
    jit_builder.symbol("$fmt.struct", ilang_runtime::__fmt_struct as *const u8);
    jit_builder.symbol("$fmt.map", ilang_runtime::__fmt_map as *const u8);
    jit_builder.symbol("$fmt.enum", ilang_runtime::__fmt_enum as *const u8);
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
    jit_builder.symbol("$type.fields", ilang_runtime::__type_fields as *const u8);
    jit_builder.symbol("$type.methods", ilang_runtime::__type_methods as *const u8);
    jit_builder.symbol("$type.parent", ilang_runtime::__type_parent as *const u8);
    jit_builder.symbol("$type.typeArgs", ilang_runtime::__type_type_args as *const u8);
    jit_builder.symbol("$type.fieldType", ilang_runtime::__type_field_type as *const u8);
    jit_builder.symbol("$type.methodReturn", ilang_runtime::__type_method_return as *const u8);
    jit_builder.symbol("$type.methodParams", ilang_runtime::__type_method_params as *const u8);
    jit_builder.symbol(
        "$type.registerDeclaredFieldCount",
        ilang_runtime::__register_type_declared_field_count as *const u8,
    );
    jit_builder.symbol(
        "$type.registerMethod",
        ilang_runtime::__register_type_method as *const u8,
    );
    jit_builder.symbol(
        "$type.registerParent",
        ilang_runtime::__register_type_parent as *const u8,
    );
    jit_builder.symbol(
        "$type.registerTypeArg",
        ilang_runtime::__register_type_arg as *const u8,
    );
    jit_builder.symbol(
        "$type.registerFieldType",
        ilang_runtime::__register_type_field_type as *const u8,
    );
    jit_builder.symbol(
        "$type.registerMethodReturn",
        ilang_runtime::__register_type_method_return as *const u8,
    );
    jit_builder.symbol(
        "$type.registerMethodParam",
        ilang_runtime::__register_type_method_param as *const u8,
    );
    jit_builder.symbol("$print.weak", ilang_runtime::__print_weak as *const u8);
    jit_builder.symbol("$print.enum", ilang_runtime::__print_enum as *const u8);
    jit_builder.symbol("$print.fn", ilang_runtime::__print_fn as *const u8);
    jit_builder.symbol("$class.releaseObject", ilang_runtime::__release_object as *const u8);
    jit_builder.symbol("$class.retainObject", ilang_runtime::__retain_object as *const u8);
    jit_builder.symbol("$weak.release", ilang_runtime::__release_weak as *const u8);
    jit_builder.symbol("$weak.retain", ilang_runtime::__retain_weak as *const u8);
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
    jit_builder.symbol("$ffi.stringFromBytes", ilang_runtime::string_from_bytes as *const u8);
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
    jit_builder.symbol("$time.set_timeout", ilang_runtime::timers::time_set_timeout as *const u8);
    jit_builder.symbol("$time.clear_timeout", ilang_runtime::timers::time_clear_timeout as *const u8);
    jit_builder.symbol("$time.set_interval", ilang_runtime::timers::time_set_interval as *const u8);
    jit_builder.symbol("$time.clear_interval", ilang_runtime::timers::time_clear_interval as *const u8);
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
    // Fixture-only helper for `fn_value_callback_pass_through.il`:
    // invokes the supplied callback once with a fixed argument
    // pattern so the fixture can compare a direct call against a
    // re-forwarded one. The MIR call site uses the bare C name
    // `register_and_invoke`, so register it with that exact key.
    jit_builder.symbol(
        "register_and_invoke",
        ilang_runtime::test_externs::register_and_invoke as *const u8,
    );
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
    jit_builder.symbol("$math.sign", ilang_runtime::math_sign as *const u8);
    jit_builder.symbol("$math.trunc", ilang_runtime::math_trunc as *const u8);
    jit_builder.symbol("$math.cbrt", ilang_runtime::math_cbrt as *const u8);
    jit_builder.symbol("$math.hypot", ilang_runtime::math_hypot as *const u8);
    jit_builder.symbol("$math.sinh", ilang_runtime::math_sinh as *const u8);
    jit_builder.symbol("$math.cosh", ilang_runtime::math_cosh as *const u8);
    jit_builder.symbol("$math.tanh", ilang_runtime::math_tanh as *const u8);
    jit_builder.symbol("$math.asinh", ilang_runtime::math_asinh as *const u8);
    jit_builder.symbol("$math.acosh", ilang_runtime::math_acosh as *const u8);
    jit_builder.symbol("$math.atanh", ilang_runtime::math_atanh as *const u8);
    jit_builder.symbol("$math.random", ilang_runtime::math_random as *const u8);
    jit_builder.symbol(
        "$math.isFinite_f32",
        ilang_runtime::math_is_finite_f32 as *const u8,
    );
    jit_builder.symbol(
        "$math.isFinite_f64",
        ilang_runtime::math_is_finite_f64 as *const u8,
    );
    jit_builder.symbol("$math.isNaN_f32", ilang_runtime::math_is_nan_f32 as *const u8);
    jit_builder.symbol("$math.hashCode_f32", ilang_runtime::math_hash_code_f32 as *const u8);
    jit_builder.symbol("$math.hashCode_f64", ilang_runtime::math_hash_code_f64 as *const u8);
    jit_builder.symbol("$math.isNaN_f64", ilang_runtime::math_is_nan_f64 as *const u8);
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
}
