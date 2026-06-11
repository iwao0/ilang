//! Program-wide function / import declaration pass.
//!
//! Walks the MIR `Program`, declares every user function plus the
//! runtime helper imports the lowerer needs (map / string / array /
//! print / panic / ARC / FFI marshalling), and finally drives
//! `lower_function` for each user-defined body.

use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::{Function as ClifFunc, Signature, UserFuncName};
use cranelift_frontend::{FunctionBuilder as ClifFnBuilder, FunctionBuilderContext};
use cranelift_module::{DataDescription, DataId, Linkage, Module};

use ilang_ast::Symbol;
use ilang_mir::{FuncId, Inst, MirConst, MirTy, Program, StaticSlotId};

use crate::ty::mir_to_clif;

use super::abi::clif_signature_for;
use super::{
    declare_binary_i64, declare_binary_i64_void, declare_quad_i64_void, declare_returns_i64,
    declare_ternary_i64, declare_ternary_i64_void, declare_unary_i64, declare_unit_f64,
    declare_unit_i64, declare_unit_void, lower_function, BuiltinDecl, CompileError, FmtIds,
    MapIds, PanicAux, PrintIds, PrintLits, PromiseIds, SetIds, StrIds,
};

pub(crate) struct LoweringOutputs {
    pub fn_ids: HashMap<FuncId, cranelift_module::FuncId>,
    pub extern_fn_ids: std::collections::HashSet<FuncId>,
    /// `@optional` extern fns whose every `@lib(...)` failed to
    /// probe — declared `Linkage::Local` so the caller can attach an
    /// abort-stub body before `module.finalize`.
    pub missing_optional_fn_ids: std::collections::HashSet<FuncId>,
    /// Extern fns that share a C symbol with an earlier declaration
    /// but were declared with a different ilang signature. Their
    /// Cranelift `FuncId` in `fn_ids` aliases the canonical
    /// declaration's `FuncId`; call sites for these must dispatch
    /// through `func_addr + call_indirect` with the per-callee
    /// signature so the wrong (canonical) signature isn't reused.
    /// Used to express the `objc_msgSend(obj, sel, ...) -> ret` family
    /// where each call shape needs its own ilang fn.
    /// Kept here for diagnostics; the in-pass `lower_function` call
    /// receives a borrow of the local set directly.
    #[allow(dead_code)]
    pub extern_alias_fn_ids: std::collections::HashSet<FuncId>,
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
    let alloc_id = declare_unary_i64(module, "$alloc.alloc")?;
    let free_id = declare_binary_i64_void(module, "$alloc.free")?;
    // Map runtime imports.
    let map_new_id = declare_returns_i64(module, "$map.new")?;
    let map_new_object_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        module.declare_function("$map.newObject", Linkage::Import, &sig)?
    };
    let map_get_id = declare_binary_i64(module, "$map.get")?;
    let map_get_optional_id = declare_binary_i64(module, "$map.getOptional")?;
    let map_set_id = declare_ternary_i64_void(module, "$map.set")?;
    let map_has_id = declare_binary_i64(module, "$map.has")?;
    let map_size_id = declare_unary_i64(module, "$map.size")?;
    let map_delete_id = declare_binary_i64(module, "$map.delete")?;
    let map_keys_id = declare_unary_i64(module, "$map.keys")?;
    let map_values_id = declare_unary_i64(module, "$map.values")?;
    let map_clear_id = declare_unit_i64(module, "$map.clear")?;
    let map_entries_id = declare_unary_i64(module, "$map.entries")?;
    // (map, closure, key_fk, val_fk) -> void.
    let map_for_each_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        module.declare_function("$map.forEach", Linkage::Import, &sig)?
    };
    // Set runtime imports — mirror Map's shape but every entry-side
    // op is a 2-arg `(set, raw_elem)` instead of `(set, key, value)`.
    let set_new_id = declare_returns_i64(module, "$set.new")?;
    // `$set.newObject(eq_fn, hash_fn) -> i64` — two i64 fn-pointer args,
    // returns the set ptr. Used by `new Set<MyClass>()` lowering.
    let set_new_object_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        module.declare_function("$set.newObject", Linkage::Import, &sig)?
    };
    let set_add_id = declare_binary_i64_void(module, "$set.add")?;
    let set_has_id = declare_binary_i64(module, "$set.has")?;
    let set_delete_id = declare_binary_i64(module, "$set.delete")?;
    let set_size_id = declare_unary_i64(module, "$set.size")?;
    let set_clear_id = declare_unit_i64(module, "$set.clear")?;
    let set_set_elem_print_kind_id =
        declare_binary_i64_void(module, "$set.setElemPrintKind")?;
    let set_add_f32_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::F32));
        module.declare_function("$set.addF32", Linkage::Import, &sig)?
    };
    let set_add_f64_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::F64));
        module.declare_function("$set.addF64", Linkage::Import, &sig)?
    };
    let set_has_f32_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::F32));
        sig.returns.push(AbiParam::new(types::I64));
        module.declare_function("$set.hasF32", Linkage::Import, &sig)?
    };
    let set_has_f64_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::F64));
        sig.returns.push(AbiParam::new(types::I64));
        module.declare_function("$set.hasF64", Linkage::Import, &sig)?
    };
    let set_delete_f32_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::F32));
        sig.returns.push(AbiParam::new(types::I64));
        module.declare_function("$set.deleteF32", Linkage::Import, &sig)?
    };
    let set_delete_f64_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::F64));
        sig.returns.push(AbiParam::new(types::I64));
        module.declare_function("$set.deleteF64", Linkage::Import, &sig)?
    };
    let set_values_id = declare_unary_i64(module, "$set.values")?;
    let set_for_each_id = declare_binary_i64_void(module, "$set.forEach")?;
    let set_for_each_f32_id = declare_binary_i64_void(module, "$set.forEachF32")?;
    let set_for_each_f64_id = declare_binary_i64_void(module, "$set.forEachF64")?;
    let set_union_id = declare_binary_i64(module, "$set.union")?;
    let set_intersection_id = declare_binary_i64(module, "$set.intersection")?;
    let set_difference_id = declare_binary_i64(module, "$set.difference")?;
    let set_is_subset_of_id = declare_binary_i64(module, "$set.isSubsetOf")?;
    let set_is_superset_of_id = declare_binary_i64(module, "$set.isSupersetOf")?;
    let set_is_disjoint_from_id = declare_binary_i64(module, "$set.isDisjointFrom")?;
    // Promise runtime imports.
    let promise_resolve_id = declare_binary_i64(module, "$promise.resolve")?;
    let promise_reject_id = declare_unary_i64(module, "$promise.reject")?;
    // (promise, on_resolve, out_kind, in_fk, out_fk) -> promise.
    let promise_then_id = {
        let mut sig = module.make_signature();
        for _ in 0..5 {
            sig.params.push(AbiParam::new(types::I64));
        }
        sig.returns.push(AbiParam::new(types::I64));
        module.declare_function("$promise.then", Linkage::Import, &sig)?
    };
    // (promise, on_reject, out_kind, out_fk) -> promise.
    let promise_catch_id = {
        let mut sig = module.make_signature();
        for _ in 0..4 {
            sig.params.push(AbiParam::new(types::I64));
        }
        sig.returns.push(AbiParam::new(types::I64));
        module.declare_function("$promise.catch", Linkage::Import, &sig)?
    };
    // (executor, value_kind, value_fk) -> promise. `value_fk` picks
    // the float-ABI resolve stub (0 = int/ptr, 1 = f32, 2 = f64).
    let promise_with_executor_id =
        declare_ternary_i64(module, "$promise.withExecutor")?;
    let promise_drain_id = declare_unit_void(module, "$promise.drain")?;
    let promise_all_id = declare_binary_i64(module, "$promise.all")?;
    let promise_race_id = declare_binary_i64(module, "$promise.race")?;
    let promise_pending_id = declare_returns_i64(module, "$promise.pending")?;
    let promise_settle_resolve_id =
        declare_ternary_i64_void(module, "$promise.settleResolve")?;
    let promise_settle_reject_id =
        declare_binary_i64_void(module, "$promise.settleReject")?;
    // (upstream, target) -> void — async desugar's awaited-rejection
    // propagation hook.
    let promise_reject_follows_id =
        declare_binary_i64_void(module, "$promise.rejectFollows")?;
    // `__ilang_make_objc_block(closure: i64, kind: i64) -> i64`.
    // Always declared even on non-macOS hosts so MIR programs that
    // mention `new ObjCBlock(...)` can still link; on those hosts
    // the runtime symbol returns 0 (the macOS-only impl is gated
    // out) and the program fails at call time.
    let make_objc_block_id =
        declare_binary_i64(module, "$objc.make_block")?;
    // ObjCBlock.invoke(args) per-shape entry points. Each declares
    // the matching C-ABI signature; calls flow through the
    // `invoke_*_block` builtin names in `lower_inst::calls`.
    let invoke_void_block_id =
        declare_unit_i64(module, "$objc.invoke_void_block")?;
    let invoke_obj_block_id = declare_binary_i64_void(module, "$objc.invoke_obj_block")?;
    let invoke_obj_to_obj_block_id =
        declare_binary_i64(module, "$objc.invoke_obj_to_obj_block")?;
    let invoke_void_bytes_block_id =
        declare_ternary_i64_void(module, "$objc.invoke_void_bytes_block")?;
    let invoke_void_three_obj_block_id =
        declare_quad_i64_void(module, "$objc.invoke_void_three_obj_block")?;
    // (i64, i8) → () — the i8 is the bool payload; one-off signature
    // so no helper.
    let invoke_void_bool_block_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I8));
        module.declare_function("$objc.invoke_void_bool_block", Linkage::Import, &sig)?
    };
    // Non-FFI internal helpers that still ride the imports path
    // (the FFI helpers themselves moved to `libs/std/ffi.il`, where
    // the user's `@intrinsic(...) @extern(C)` declarations carry
    // their own cranelift declarations).
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
        decl_unary("$array.dataPtr", false)?;
        decl_unary("$enum.box", false)?;
    }
    // `arrayFromCArray<T>` — user-facing surface lives in
    // `libs/std/ffi.il` as `@intrinsic("ffi.arrayFromCArray")` with a
    // 2-arg generic signature; the runtime helper takes the 4-arg
    // `(src, n, stride, kind_tag)` form and the MIR special case
    // (call_fn.rs) synthesises stride / kind_tag from the call-site
    // pointer type before dispatching here. The `(FuncId, Signature)`
    // is also stashed into `builtin_ids` below (key
    // "c_array_to_array") so the `NewObject` codegen can call it
    // directly to synthesise empty-array defaults for declared
    // `T[]` fields.
    let c_array_to_array_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        let id = module
            .declare_function("$ffi.arrayFromCArray", Linkage::Import, &sig)?;
        (id, sig)
    };
    // `$ffi.cstrFromString` — parser-synthesised by the @objc desugar
    // (see ilang-parser `extern_objc::build_cstr`). Declared as an
    // Import here so the bare-name lookup path in MIR codegen
    // (`lower_inst/calls.rs`) resolves it without depending on whether
    // any user module imported `std.ffi { cstrFromString }`.
    {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        module.declare_function("$ffi.cstrFromString", Linkage::Import, &sig)?;
    }
    // `$ffi.readU64` — used by MIR codegen to extract the raw
    // fn pointer (offset 0) from an ilang closure box when an
    // `@extern(C)` callee is handed an `fn(...)` value that's
    // already shaped as a closure (a re-forwarded callback).
    // Always declared so the lookup in `lower_inst/calls.rs`
    // resolves whether or not user code imported
    // `std.ffi { readU64 }`.
    {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        module.declare_function("$ffi.readU64", Linkage::Import, &sig)?;
    }
    // REPL slot accessors. Loaded as imports so chunk-level
    // compilations don't need a fresh declaration; the host symbol
    // table provides the bodies via `JITBuilder::symbol`.
    {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        module.declare_function("$repl.loadSlot", Linkage::Import, &sig)?;
    }
    {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        module.declare_function("$repl.storeSlot", Linkage::Import, &sig)?;
    }
    let str_ids = StrIds {
        length: declare_unary_i64(module, "$string.length")?,
        concat: declare_binary_i64(module, "$string.concat")?,
        concat_inplace: declare_binary_i64(module, "$string.concatInplace")?,
        eq: declare_binary_i64(module, "$string.eq")?,
        int_to_string: declare_unary_i64(module, "$string.fromInt")?,
        bool_to_string: declare_unary_i64(module, "$string.fromBool")?,
        to_upper: declare_unary_i64(module, "$string.toUpper")?,
        to_lower: declare_unary_i64(module, "$string.toLower")?,
        trim: declare_unary_i64(module, "$string.trim")?,
        includes: declare_binary_i64(module, "$string.includes")?,
        starts_with: declare_binary_i64(module, "$string.startsWith")?,
        ends_with: declare_binary_i64(module, "$string.endsWith")?,
        char_at: declare_binary_i64(module, "$string.charAt")?,
        slice: {
            let mut sig = module.make_signature();
            sig.params.push(AbiParam::new(types::I64));
            sig.params.push(AbiParam::new(types::I64));
            sig.params.push(AbiParam::new(types::I64));
            sig.returns.push(AbiParam::new(types::I64));
            module.declare_function("$string.slice", Linkage::Import, &sig)?
        },
        replace: {
            let mut sig = module.make_signature();
            sig.params.push(AbiParam::new(types::I64));
            sig.params.push(AbiParam::new(types::I64));
            sig.params.push(AbiParam::new(types::I64));
            sig.returns.push(AbiParam::new(types::I64));
            module.declare_function("$string.replace", Linkage::Import, &sig)?
        },
        index_of: declare_ternary_i64(module, "$string.indexOf")?,
        last_index_of: declare_ternary_i64(module, "$string.lastIndexOf")?,
        encode_utf16: declare_binary_i64(module, "$string.encodeUtf16")?,
        hash_code: declare_unary_i64(module, "$string.hashCode")?,
        from_utf16: declare_unary_i64(module, "$string.fromUtf16")?,
        array_index_of: declare_binary_i64(module, "$array.indexOf")?,
        array_includes: declare_binary_i64(module, "$array.includes")?,
        array_push: {
            let mut sig = module.make_signature();
            sig.params.push(AbiParam::new(types::I64));
            sig.params.push(AbiParam::new(types::I64));
            module.declare_function("$array.push", Linkage::Import, &sig)?
        },
        array_pop: declare_unary_i64(module, "$array.pop")?,
        array_remove: declare_binary_i64(module, "$array.remove")?,
        array_remove_at: declare_binary_i64(module, "$array.removeAt")?,
        array_map: {
            let mut sig = module.make_signature();
            sig.params.push(AbiParam::new(types::I64)); // arr
            sig.params.push(AbiParam::new(types::I64)); // closure
            sig.params.push(AbiParam::new(types::I64)); // result_kind
            sig.params.push(AbiParam::new(types::I64)); // result_stride
            sig.params.push(AbiParam::new(types::I64)); // arg_fk
            sig.params.push(AbiParam::new(types::I64)); // ret_fk
            sig.returns.push(AbiParam::new(types::I64));
            module.declare_function("$array.map", Linkage::Import, &sig)?
        },
        array_filter: declare_ternary_i64(module, "$array.filter")?,
        array_for_each: {
            let mut sig = module.make_signature();
            sig.params.push(AbiParam::new(types::I64)); // arr
            sig.params.push(AbiParam::new(types::I64)); // closure
            sig.params.push(AbiParam::new(types::I64)); // arg_fk
            module.declare_function("$array.forEach", Linkage::Import, &sig)?
        },
        array_slice: {
            let mut sig = module.make_signature();
            sig.params.push(AbiParam::new(types::I64));
            sig.params.push(AbiParam::new(types::I64));
            sig.params.push(AbiParam::new(types::I64));
            sig.returns.push(AbiParam::new(types::I64));
            module.declare_function("$array.slice", Linkage::Import, &sig)?
        },
        array_find: declare_ternary_i64(module, "$array.find")?,
        array_find_index: declare_ternary_i64(module, "$array.findIndex")?,
        array_every: declare_ternary_i64(module, "$array.every")?,
        array_some: declare_ternary_i64(module, "$array.some")?,
        array_concat: declare_binary_i64(module, "$array.concat")?,
        array_reverse: declare_unary_i64(module, "$array.reverse")?,
        array_join: declare_binary_i64(module, "$array.join")?,
        array_shift: declare_unary_i64(module, "$array.shift")?,
        array_unshift: {
            let mut sig = module.make_signature();
            sig.params.push(AbiParam::new(types::I64));
            sig.params.push(AbiParam::new(types::I64));
            module.declare_function("$array.unshift", Linkage::Import, &sig)?
        },
        array_fill: {
            let mut sig = module.make_signature();
            sig.params.push(AbiParam::new(types::I64));
            sig.params.push(AbiParam::new(types::I64));
            module.declare_function("$array.fill", Linkage::Import, &sig)?
        },
        array_sort: declare_ternary_i64(module, "$array.sort")?,
        str_split: declare_binary_i64(module, "$string.split")?,
        virt_dispatch: declare_binary_i64(module, "$class.virtDispatch")?,
        fixed_to_dyn: {
            let mut sig = module.make_signature();
            sig.params.push(AbiParam::new(types::I64));
            sig.params.push(AbiParam::new(types::I64));
            sig.params.push(AbiParam::new(types::I64));
            sig.params.push(AbiParam::new(types::I64));
            sig.returns.push(AbiParam::new(types::I64));
            module.declare_function("$array.fixedToDyn", Linkage::Import, &sig)?
        },
    };
    let panic_fn_id = declare_unit_i64(module, "$ilang.panic")?;
    let drop_dispatch_id = declare_unary_i64(module, "$class.dropDispatch")?;
    let print_object_id = declare_unit_i64(module, "$print.object")?;
    let print_struct_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        module.declare_function("$print.struct", Linkage::Import, &sig)?
    };
    let print_fn_id = declare_unit_i64(module, "$print.fn")?;
    let release_obj_id = declare_unit_i64(module, "$class.releaseObject")?;
    let retain_obj_id = declare_unit_i64(module, "$class.retainObject")?;
    let release_weak_id = declare_unit_i64(module, "$weak.release")?;
    let retain_weak_id = declare_unit_i64(module, "$weak.retain")?;
    let release_closure_id = declare_unit_i64(module, "$closure.release")?;
    let retain_closure_id = declare_unit_i64(module, "$closure.retain")?;
    let release_array_id = declare_unit_i64(module, "$array.release")?;
    let retain_array_id = declare_unit_i64(module, "$array.retain")?;
    let release_optional_id = declare_unit_i64(module, "$optional.release")?;
    let retain_optional_id = declare_unit_i64(module, "$optional.retain")?;
    let release_tuple_id = declare_unit_i64(module, "$tuple.release")?;
    let retain_tuple_id = declare_unit_i64(module, "$tuple.retain")?;
    let release_map_id = declare_unit_i64(module, "$map.release")?;
    let retain_map_id = declare_unit_i64(module, "$map.retain")?;
    let release_set_id = declare_unit_i64(module, "$set.release")?;
    let retain_set_id = declare_unit_i64(module, "$set.retain")?;
    let release_string_id = declare_unit_i64(module, "$string.release")?;
    let retain_string_id = declare_unit_i64(module, "$string.retain")?;
    let enum_unit_get_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        module.declare_function("$enum.unitGet", Linkage::Import, &sig)?
    };
    let enum_unit_get_checked_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        module.declare_function("$enum.unitGetChecked", Linkage::Import, &sig)?
    };
    let enum_disc_str_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        module.declare_function("$enum.discStr", Linkage::Import, &sig)?
    };
    let enum_alloc_id = declare_ternary_i64(module, "$enum.alloc")?;
    let release_enum_id = declare_unit_i64(module, "$enum.release")?;
    let retain_enum_id = declare_unit_i64(module, "$enum.retain")?;
    let release_promise_id = declare_unit_i64(module, "$promise.release")?;
    let retain_promise_id = declare_unit_i64(module, "$promise.retain")?;
    let map_set_val_kind_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        module.declare_function("$map.setValueKind", Linkage::Import, &sig)?
    };
    let map_set_print_kinds_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        module.declare_function("$map.setPrintKinds", Linkage::Import, &sig)?
    };
    let print_map_id = declare_unit_i64(module, "$print.map")?;
    let print_set_id = declare_unit_i64(module, "$print.set")?;
    let class_name_id = declare_unary_i64(module, "$class.name")?;
    let print_weak_id = declare_unit_i64(module, "$print.weak")?;
    let print_enum_id = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        module.declare_function("$print.enum", Linkage::Import, &sig)?
    };
    let print_ids = PrintIds {
        int: declare_unit_i64(module, "$print.int")?,
        bool_: declare_unit_i64(module, "$print.bool")?,
        f64_: declare_unit_f64(module, "$print.f64")?,
        str_: declare_unit_i64(module, "$print.str")?,
        space: declare_unit_void(module, "$print.space")?,
        newline: declare_unit_void(module, "$print.newline")?,
        object: print_object_id,
        struct_: print_struct_id,
        fn_: print_fn_id,
        map: print_map_id,
        set: print_set_id,
        weak: print_weak_id,
        enum_: print_enum_id,
        promise: declare_unit_i64(module, "$print.promise")?,
    };
    let fmt_ids = FmtIds {
        int: declare_unary_i64(module, "$fmt.int")?,
        bool_: declare_unary_i64(module, "$fmt.bool")?,
        f64_: {
            // f64 → string. Signature differs from the unary-i64 helper.
            let mut sig = module.make_signature();
            sig.params.push(AbiParam::new(types::F64));
            sig.returns.push(AbiParam::new(types::I64));
            module.declare_function("$fmt.f64", Linkage::Import, &sig)?
        },
        str_: declare_unary_i64(module, "$fmt.str")?,
        weak: declare_unary_i64(module, "$fmt.weak")?,
        fn_: declare_unary_i64(module, "$fmt.fn")?,
        object: declare_unary_i64(module, "$fmt.object")?,
        struct_: declare_binary_i64(module, "$fmt.struct")?,
        map: declare_unary_i64(module, "$fmt.map")?,
        promise: declare_unary_i64(module, "$fmt.promise")?,
        set: declare_unary_i64(module, "$fmt.set")?,
        enum_: declare_binary_i64(module, "$fmt.enum")?,
    };

    // Declare builtin imports. Each gets a Cranelift FuncId so call
    // sites can resolve via `module.declare_func_in_func`.
    let mut builtin_ids: HashMap<String, (cranelift_module::FuncId, Signature)> =
        HashMap::new();
    builtin_ids.insert(
        "c_array_to_array".to_string(),
        c_array_to_array_id,
    );
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
    // `(f32).isFinite()` / `(f64).isFinite()` / matching isNaN — the
    // MIR side emits these as `FuncRef::Builtin` calls so they fall
    // through the host_id table into `builtin_ids`. Per-width entries
    // because cranelift's float-arg ABI distinguishes f32 from f64.
    {
        let mut s_f32 = module.make_signature();
        s_f32.params.push(AbiParam::new(types::F32));
        s_f32.returns.push(AbiParam::new(types::I64));
        let cid = module.declare_function("$math.isFinite_f32", Linkage::Import, &s_f32)?;
        builtin_ids.insert("math_is_finite_f32".to_string(), (cid, s_f32.clone()));
        let cid = module.declare_function("$math.isNaN_f32", Linkage::Import, &s_f32)?;
        builtin_ids.insert("math_is_nan_f32".to_string(), (cid, s_f32.clone()));

        let mut s_f64 = module.make_signature();
        s_f64.params.push(AbiParam::new(types::F64));
        s_f64.returns.push(AbiParam::new(types::I64));
        let cid = module.declare_function("$math.isFinite_f64", Linkage::Import, &s_f64)?;
        builtin_ids.insert("math_is_finite_f64".to_string(), (cid, s_f64.clone()));
        let cid = module.declare_function("$math.isNaN_f64", Linkage::Import, &s_f64)?;
        builtin_ids.insert("math_is_nan_f64".to_string(), (cid, s_f64.clone()));

        // `(f32).hashCode()` / `(f64).hashCode()` — reinterpret the
        // float's bit pattern as an i64 (sign-extended for f32 so the
        // result is a stable 64-bit identifier).
        let cid = module.declare_function("$math.hashCode_f32", Linkage::Import, &s_f32)?;
        builtin_ids.insert("math_hash_code_f32".to_string(), (cid, s_f32.clone()));
        let cid = module.declare_function("$math.hashCode_f64", Linkage::Import, &s_f64)?;
        builtin_ids.insert("math_hash_code_f64".to_string(), (cid, s_f64.clone()));

        // `(f32).toString()` / `(f64).toString()` — same per-width split as
        // isFinite / isNaN; result is the ilang string pointer (i64).
        let cid = module.declare_function("$string.fromF32", Linkage::Import, &s_f32)?;
        builtin_ids.insert("float_to_string_f32".to_string(), (cid, s_f32));
        let cid = module.declare_function("$string.fromF64", Linkage::Import, &s_f64)?;
        builtin_ids.insert("float_to_string_f64".to_string(), (cid, s_f64));
    }

    // Reflection builtins fed by the `typeof(x).<member>` lowering.
    // Each takes the class id (i64) and returns a heap value (i64).
    {
        let mut unary_sig = module.make_signature();
        unary_sig.params.push(AbiParam::new(types::I64));
        unary_sig.returns.push(AbiParam::new(types::I64));
        let mut binary_sig = module.make_signature();
        binary_sig.params.push(AbiParam::new(types::I64));
        binary_sig.params.push(AbiParam::new(types::I64));
        binary_sig.returns.push(AbiParam::new(types::I64));

        let cid = declare_unary_i64(module, "$type.fields")?;
        builtin_ids.insert("type_fields".to_string(), (cid, unary_sig.clone()));
        let cid = declare_unary_i64(module, "$type.kind")?;
        builtin_ids.insert("type_kind".to_string(), (cid, unary_sig.clone()));
        let cid = declare_unary_i64(module, "$type.methods")?;
        builtin_ids.insert("type_methods".to_string(), (cid, unary_sig.clone()));
        let cid = declare_unary_i64(module, "$type.parent")?;
        builtin_ids.insert("type_parent".to_string(), (cid, unary_sig.clone()));
        let cid = declare_unary_i64(module, "$type.typeArgs")?;
        builtin_ids.insert("type_typeargs".to_string(), (cid, unary_sig));

        // `(class_id, name_ptr) -> i64`.
        let cid = module.declare_function("$type.fieldType", Linkage::Import, &binary_sig)?;
        builtin_ids.insert("type_field_type".to_string(), (cid, binary_sig.clone()));
        let cid = module.declare_function("$type.methodReturn", Linkage::Import, &binary_sig)?;
        builtin_ids.insert("type_method_return".to_string(), (cid, binary_sig.clone()));
        let cid = module.declare_function("$type.methodParams", Linkage::Import, &binary_sig)?;
        builtin_ids.insert("type_method_params".to_string(), (cid, binary_sig));
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
                        let mut bytes: Vec<u8> = Vec::with_capacity(24 + body.len() + 1);
                        // `[ i64 cap=0 | i64 rc=-1 | i64 len | bytes
                        //   | \0 ]`. cap=0 + rc=-1 mark this as an
                        // immutable static literal: runtime retain /
                        // release (which both skip on `rc <= 0`) are
                        // no-ops, and `__release_string` never tries
                        // to dealloc.
                        bytes.extend_from_slice(&0i64.to_le_bytes());
                        bytes.extend_from_slice(&(-1i64).to_le_bytes());
                        bytes.extend_from_slice(&(body.len() as i64).to_le_bytes());
                        bytes.extend_from_slice(body);
                        bytes.push(0);
                        let mut desc = DataDescription::new();
                        // Align=8 — the body pointer reads
                        // `*((ptr - 8) as *const i64)` for length,
                        // `*((ptr - 16) as *const i64)` for rc, and
                        // `*((ptr - 24) as *const i64)` for cap.
                        // Without explicit alignment Cranelift packs
                        // data segments at byte alignment, tripping
                        // Rust's misaligned-pointer check at runtime.
                        desc.set_align(8);
                        desc.define(bytes.into_boxed_slice());
                        let name = format!("$str.{}", next_str_id);
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
    // Same `[ i64 cap=0 | i64 rc=-1 | i64 length | bytes | \0 ]`
    // shape as user string literals — keeps cstr_bytes /
    // host_ilang_panic / host_print_str happy without per-call-site
    // special-casing. Consumers add 24 to the symbol address to get
    // the user-visible pointer.
    let mut declare_msg = |name: &str, text: &str| -> Result<DataId, CompileError> {
        let body = text.as_bytes();
        let mut bytes: Vec<u8> = Vec::with_capacity(24 + body.len() + 1);
        bytes.extend_from_slice(&0i64.to_le_bytes());
        bytes.extend_from_slice(&(-1i64).to_le_bytes());
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
    let panic_msg_div = declare_msg("$panic.msgDiv", "panic: division by zero")?;
    let panic_msg_mod = declare_msg("$panic.msgMod", "panic: modulo by zero / division by zero")?;
    let panic_msg_oob = declare_msg("$panic.msgOob", "panic: index out of bounds")?;
    let panic_msg_unwrap = declare_msg("$panic.msgUnwrap", "panic: unwrap of None")?;
    let lit_none = declare_msg("$lit.none", "none")?;
    let lit_some_open = declare_msg("$lit.someOpen", "some(")?;
    let lit_close_paren = declare_msg("$lit.cparen", ")")?;
    let lit_open_paren = declare_msg("$lit.oparen", "(")?;
    let lit_open_bracket = declare_msg("$lit.obracket", "[")?;
    let lit_close_bracket = declare_msg("$lit.cbracket", "]")?;
    let lit_comma_sp = declare_msg("$lit.commaSp", ", ")?;

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
        let name = format!("$static.{}", s.id.0);
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
    let mut extern_alias_fn_ids: std::collections::HashSet<FuncId> =
        std::collections::HashSet::new();
    // Track the first Cranelift FuncId per C symbol so subsequent
    // ilang declarations with a different signature can alias it
    // (the call sites dispatch through `func_addr + call_indirect`).
    let mut c_sym_canonical: HashMap<String, cranelift_module::FuncId> = HashMap::new();
    // Pre-populate with the runtime symbols already declared above
    // (`$alloc.alloc`, `$ffi.arrayFromCArray`, …). A user-facing
    // `@intrinsic("...")` FnDecl whose c_symbol resolves to one of
    // those names — e.g. `arrayFromCArray<T>(p, n): T[]` whose runtime
    // helper takes `(p, n, stride, kind)` — must alias the existing
    // declaration instead of trying to re-declare with a conflicting
    // signature; the alias path keeps the type-checker's user-facing
    // signature while the MIR special case dispatches through
    // `FuncRef::Builtin` directly.
    for pre in [
        "$alloc.alloc",
        "$alloc.free",
        "$map.new",
        "$map.get",
        "$map.getOptional",
        "$map.set",
        "$map.has",
        "$promise.drain",
        "$promise.pending",
        "$promise.settleResolve",
        "$promise.settleReject",
        "$ffi.arrayFromCArray",
        "$ffi.cstrFromString",
        "$repl.loadSlot",
        "$repl.storeSlot",
    ] {
        if let Some(cranelift_module::FuncOrDataId::Func(cid)) =
            module.declarations().get_name(pre)
        {
            c_sym_canonical.insert(pre.to_string(), cid);
        }
    }
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
        let is_extern = matches!(func.kind, ilang_mir::FunctionKind::Extern { .. });
        if is_extern {
            extern_fn_ids.insert(mid);
        }
        // Alias path: a previous extern declaration already pinned
        // `symbol_name` to a Cranelift FuncId. Cranelift only allows
        // one signature per symbol; we reuse the existing FuncId for
        // the address lookup and flag this MIR fn so the call
        // lowering emits `call_indirect` with our own signature.
        if is_extern {
            if let Some(&existing_cid) = c_sym_canonical.get(symbol_name) {
                fn_ids.insert(mid, existing_cid);
                fn_sigs.insert(mid, sig);
                extern_alias_fn_ids.insert(mid);
                continue;
            }
        }
        let linkage = if is_extern {
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
        } else if symbol_name.starts_with("$objc.imp.") {
            // Parser-synthesised IMPs for `@objc class : Parent`
            // overrides need to be discoverable from the runtime
            // (`dlsym(RTLD_DEFAULT, ...)`) so the generated
            // `register()` body can hand their addresses to
            // `class_addMethod`. Tagging by the canonical name
            // prefix avoids threading a new flag through MIR.
            Linkage::Export
        } else {
            Linkage::Local
        };
        let cid = module.declare_function(symbol_name, linkage, &sig)?;
        fn_ids.insert(mid, cid);
        fn_sigs.insert(mid, sig);
        if is_extern {
            c_sym_canonical.insert(symbol_name.to_string(), cid);
        }
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
                new_object: map_new_object_id,
                get: map_get_id,
                get_optional: map_get_optional_id,
                set: map_set_id,
                size: map_size_id,
                has: map_has_id,
                delete: map_delete_id,
                keys: map_keys_id,
                values: map_values_id,
                clear: map_clear_id,
                entries: map_entries_id,
                for_each: map_for_each_id,
            };
            let set_ids = SetIds {
                new: set_new_id,
                new_object: set_new_object_id,
                add: set_add_id,
                has: set_has_id,
                delete: set_delete_id,
                size: set_size_id,
                clear: set_clear_id,
                set_elem_print_kind: set_set_elem_print_kind_id,
                add_f32: set_add_f32_id,
                add_f64: set_add_f64_id,
                has_f32: set_has_f32_id,
                has_f64: set_has_f64_id,
                delete_f32: set_delete_f32_id,
                delete_f64: set_delete_f64_id,
                values: set_values_id,
                for_each: set_for_each_id,
                for_each_f32: set_for_each_f32_id,
                for_each_f64: set_for_each_f64_id,
                union: set_union_id,
                intersection: set_intersection_id,
                difference: set_difference_id,
                is_subset_of: set_is_subset_of_id,
                is_superset_of: set_is_superset_of_id,
                is_disjoint_from: set_is_disjoint_from_id,
            };
            let promise_ids = PromiseIds {
                resolve: promise_resolve_id,
                reject: promise_reject_id,
                then: promise_then_id,
                catch: promise_catch_id,
                with_executor: promise_with_executor_id,
                drain: promise_drain_id,
                all: promise_all_id,
                race: promise_race_id,
                pending: promise_pending_id,
                settle_resolve: promise_settle_resolve_id,
                settle_reject: promise_settle_reject_id,
                reject_follows: promise_reject_follows_id,
            };
            let panic_aux = PanicAux {
                fn_id: panic_fn_id,
                drop_dispatch: drop_dispatch_id,
                release_obj: release_obj_id,
                retain_obj: retain_obj_id,
                release_weak: release_weak_id,
                retain_weak: retain_weak_id,
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
                release_set: release_set_id,
                retain_set: retain_set_id,
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
                release_promise: release_promise_id,
                retain_promise: retain_promise_id,
                make_objc_block: make_objc_block_id,
                invoke_void_block: invoke_void_block_id,
                invoke_obj_block: invoke_obj_block_id,
                invoke_obj_to_obj_block: invoke_obj_to_obj_block_id,
                invoke_void_bytes_block: invoke_void_bytes_block_id,
                invoke_void_three_obj_block: invoke_void_three_obj_block_id,
                invoke_void_bool_block: invoke_void_bool_block_id,
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
                &extern_alias_fn_ids,
                &builtin_ids,
                &static_data,
                &string_data,
                alloc_id,
                map_ids,
                set_ids,
                promise_ids,
                str_ids,
                print_ids,
                fmt_ids,
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
    Ok(LoweringOutputs {
        fn_ids,
        extern_fn_ids,
        missing_optional_fn_ids,
        extern_alias_fn_ids,
    })
}
