//! Compile a MIR `Program` into a Cranelift JIT module and invoke
//! the entry function.
//!
//! Currently restricted to programs whose values are all primitive
//! scalars (integers / floats / bool / unit). Heap, ARC, FFI, and
//! virtual dispatch land alongside their MIR features in follow-up
//! steps.

mod abi;
mod binop_cast;
mod host_misc;
mod host_os;
mod jit_setup;
mod lower_function;
mod lower_inst;
mod lower_term_const;
mod print_emit;
mod print_kind;
mod program_decl;

pub use jit_setup::{compile_program, compile_with_builtins};
pub(crate) use program_decl::{
    lower_program_into, lower_program_into_with_missing, LoweringOutputs,
};
use lower_function::lower_function;

use cranelift::prelude::*;
use cranelift_codegen::ir::{AbiParam, InstBuilder};
use cranelift_frontend::FunctionBuilder as ClifFnBuilder;
use cranelift_jit::JITModule;
use cranelift_module::{Linkage, Module};

use ilang_mir::{ClassId, MirTy, Program};

#[derive(Clone, Copy)]
pub(super) struct MapIds {
    pub(super) new: cranelift_module::FuncId,
    pub(super) get: cranelift_module::FuncId,
    pub(super) get_optional: cranelift_module::FuncId,
    pub(super) set: cranelift_module::FuncId,
    pub(super) size: cranelift_module::FuncId,
    pub(super) has: cranelift_module::FuncId,
    pub(super) delete: cranelift_module::FuncId,
    pub(super) keys: cranelift_module::FuncId,
    pub(super) values: cranelift_module::FuncId,
}

#[derive(Clone, Copy)]
pub(super) struct PromiseIds {
    pub(super) resolve: cranelift_module::FuncId,
    pub(super) reject: cranelift_module::FuncId,
    pub(super) then: cranelift_module::FuncId,
    pub(super) catch: cranelift_module::FuncId,
    pub(super) with_executor: cranelift_module::FuncId,
    pub(super) drain: cranelift_module::FuncId,
    pub(super) all: cranelift_module::FuncId,
    pub(super) race: cranelift_module::FuncId,
    pub(super) pending: cranelift_module::FuncId,
    pub(super) settle_resolve: cranelift_module::FuncId,
    pub(super) settle_reject: cranelift_module::FuncId,
}

#[derive(Clone, Copy)]
pub(super) struct StrIds {
    pub(super) length: cranelift_module::FuncId,
    pub(super) concat: cranelift_module::FuncId,
    pub(super) concat_inplace: cranelift_module::FuncId,
    pub(super) eq: cranelift_module::FuncId,
    pub(super) int_to_string: cranelift_module::FuncId,
    pub(super) bool_to_string: cranelift_module::FuncId,
    pub(super) to_upper: cranelift_module::FuncId,
    pub(super) to_lower: cranelift_module::FuncId,
    pub(super) trim: cranelift_module::FuncId,
    pub(super) includes: cranelift_module::FuncId,
    pub(super) starts_with: cranelift_module::FuncId,
    pub(super) ends_with: cranelift_module::FuncId,
    pub(super) char_at: cranelift_module::FuncId,
    pub(super) slice: cranelift_module::FuncId,
    pub(super) replace: cranelift_module::FuncId,
    pub(super) array_index_of: cranelift_module::FuncId,
    pub(super) array_includes: cranelift_module::FuncId,
    pub(super) array_push: cranelift_module::FuncId,
    pub(super) array_pop: cranelift_module::FuncId,
    pub(super) array_map: cranelift_module::FuncId,
    pub(super) array_filter: cranelift_module::FuncId,
    pub(super) array_for_each: cranelift_module::FuncId,
    pub(super) array_slice: cranelift_module::FuncId,
    pub(super) str_split: cranelift_module::FuncId,
    pub(super) virt_dispatch: cranelift_module::FuncId,
    pub(super) fixed_to_dyn: cranelift_module::FuncId,
}

pub(super) fn declare_unary_i64<M: Module>(
    module: &mut M,
    name: &str,
) -> Result<cranelift_module::FuncId, CompileError> {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(types::I64));
    Ok(module.declare_function(name, Linkage::Import, &sig)?)
}

pub(super) fn declare_binary_i64<M: Module>(
    module: &mut M,
    name: &str,
) -> Result<cranelift_module::FuncId, CompileError> {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64));
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(types::I64));
    Ok(module.declare_function(name, Linkage::Import, &sig)?)
}

pub(super) fn declare_ternary_i64<M: Module>(
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

pub(super) fn declare_unit_i64<M: Module>(
    module: &mut M,
    name: &str,
) -> Result<cranelift_module::FuncId, CompileError> {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64));
    Ok(module.declare_function(name, Linkage::Import, &sig)?)
}

pub(super) fn declare_unit_f64<M: Module>(
    module: &mut M,
    name: &str,
) -> Result<cranelift_module::FuncId, CompileError> {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::F64));
    Ok(module.declare_function(name, Linkage::Import, &sig)?)
}

pub(super) fn declare_unit_void<M: Module>(
    module: &mut M,
    name: &str,
) -> Result<cranelift_module::FuncId, CompileError> {
    let sig = module.make_signature();
    Ok(module.declare_function(name, Linkage::Import, &sig)?)
}

#[derive(Clone, Copy)]
pub(super) struct PrintIds {
    pub(super) int: cranelift_module::FuncId,
    pub(super) bool_: cranelift_module::FuncId,
    pub(super) f64_: cranelift_module::FuncId,
    pub(super) str_: cranelift_module::FuncId,
    pub(super) space: cranelift_module::FuncId,
    pub(super) newline: cranelift_module::FuncId,
    pub(super) object: cranelift_module::FuncId,
    pub(super) struct_: cranelift_module::FuncId,
    pub(super) fn_: cranelift_module::FuncId,
    pub(super) map: cranelift_module::FuncId,
    pub(super) weak: cranelift_module::FuncId,
    pub(super) enum_: cranelift_module::FuncId,
}

#[derive(Clone, Copy)]
#[allow(dead_code)] // `drop_dispatch` / `print_map` are only consumed via runtime symbol resolution today, but kept on this aggregate so future codegen sites can reach for them without re-plumbing.
pub(super) struct PanicAux {
    pub(super) fn_id: cranelift_module::FuncId,
    pub(super) drop_dispatch: cranelift_module::FuncId,
    pub(super) release_obj: cranelift_module::FuncId,
    pub(super) retain_obj: cranelift_module::FuncId,
    pub(super) release_closure: cranelift_module::FuncId,
    pub(super) retain_closure: cranelift_module::FuncId,
    pub(super) release_array: cranelift_module::FuncId,
    pub(super) retain_array: cranelift_module::FuncId,
    pub(super) release_optional: cranelift_module::FuncId,
    pub(super) retain_optional: cranelift_module::FuncId,
    pub(super) release_tuple: cranelift_module::FuncId,
    pub(super) retain_tuple: cranelift_module::FuncId,
    pub(super) release_map: cranelift_module::FuncId,
    pub(super) retain_map: cranelift_module::FuncId,
    pub(super) map_set_val_kind: cranelift_module::FuncId,
    pub(super) map_set_print_kinds: cranelift_module::FuncId,
    pub(super) print_map: cranelift_module::FuncId,
    pub(super) class_name: cranelift_module::FuncId,
    /// `__mir_free(ptr, size)` — drops a previously-`mir_alloc`'d
    /// block. Used by `Inst::Release` for CRepr structs (which
    /// have no rc header but still need their backing buffer
    /// freed when they fall out of scope).
    pub(super) mir_free: cranelift_module::FuncId,
    pub(super) release_string: cranelift_module::FuncId,
    pub(super) retain_string: cranelift_module::FuncId,
    pub(super) enum_unit_get: cranelift_module::FuncId,
    pub(super) enum_unit_get_checked: cranelift_module::FuncId,
    pub(super) enum_disc_str: cranelift_module::FuncId,
    pub(super) enum_alloc: cranelift_module::FuncId,
    pub(super) release_enum: cranelift_module::FuncId,
    pub(super) retain_enum: cranelift_module::FuncId,
    pub(super) release_promise: cranelift_module::FuncId,
    pub(super) retain_promise: cranelift_module::FuncId,
    /// `__ilang_make_objc_block(closure, kind) -> i64` — wraps an
    /// ilang closure as an Objective-C block via the per-shape
    /// invoke trampolines living in `ilang_runtime::objc_blocks`.
    /// Called from `new ObjCBlock(closure)` lowering.
    pub(super) make_objc_block: cranelift_module::FuncId,
    pub(super) msg_div: DataId,
    pub(super) msg_mod: DataId,
    pub(super) msg_oob: DataId,
    pub(super) msg_unwrap: DataId,
}

#[derive(Clone, Copy)]
pub(super) struct PrintLits {
    pub(super) none: DataId,
    pub(super) some_open: DataId,
    pub(super) close_paren: DataId,
    pub(super) open_paren: DataId,
    pub(super) open_bracket: DataId,
    pub(super) close_bracket: DataId,
    pub(super) comma_sp: DataId,
}

/// Bytes prepended to every heap object: holds the `ClassId` so RTTI
/// (`is_instance`, `as?`, `typeof`) can recover the dynamic class.
pub(super) const OBJECT_HEADER_BYTES: i32 = 16;
use cranelift_module::DataId;

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



/// Run the program's entry fn (assumed to be `() -> i64`) and return
/// the integer return value.
pub fn run_main(c: &Compiled) -> i64 {
    let ptr = c.module.get_finalized_function(c.entry);
    let f: extern "C" fn() -> i64 = unsafe { std::mem::transmute(ptr) };
    let rc = f();
    // Drain the Promise / pool tasks that the program scheduled so
    // pending `.then` / executor bodies actually run before exit.
    // No-op if the user never touched a Promise (the pool stays
    // un-initialised).
    ilang_runtime::__promise_drain();
    rc
}

/// Emit a boolean expression equivalent to `class_id ∈ {target ∪ all
/// transitive subclasses of target}`. Implements `is_instance` /
/// downcast eligibility for the language's single-inheritance model.
pub(super) fn emit_is_subclass(
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
pub(super) fn walk_mir_ty(ty: &MirTy, f: &mut impl FnMut(&MirTy)) {
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






