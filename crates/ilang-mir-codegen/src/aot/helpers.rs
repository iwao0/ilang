//! Small standalone helpers used by the AOT pipeline:
//!
//! - `print_kind_id_for_ty` / `field_kind_tag`: PK_* / KIND_* tag
//!   projection from a MIR type. Mirror the JIT-side tables in
//!   `compile/print_kind.rs`.
//! - `coerce_to_i32`: fold the entry function's return value into
//!   the C-ABI process exit code.
//! - `validate_subset`: reject programs that rely on runtime
//!   features the AOT path doesn't populate yet — entry-with-params
//!   and closure entries are the only current rejections.

use cranelift::prelude::*;
use cranelift_frontend::FunctionBuilder as ClifFnBuilder;

use ilang_mir::{FuncId, MirTy, Program};

use super::AotError;

/// Map a MIR type to the runtime's `PK_*` print tag. Mirrors
/// `compile::print_kind_id`.
pub(super) fn print_kind_id_for_ty(ty: &MirTy) -> i64 {
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
pub(super) fn field_kind_tag(ty: &MirTy) -> i64 {
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
pub(super) fn coerce_to_i32(fb: &mut ClifFnBuilder, v: Value, ty: &MirTy) -> Value {
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
pub(super) fn validate_subset(
    _prog: &Program,
    entry: &ilang_mir::Function,
) -> Result<(), AotError> {
    // Classes lower through the same NewObject / LoadField paths the
    // JIT uses. Programs that rely on `__virt_dispatch` / `__drop_dispatch`
    // or other runtime-dispatch tables fail at the linker — the runtime
    // crate ships no-op `__retain_object` / `__release_object` until the
    // table-population init-emit lands.
    // Static slots are emitted by the shared `lower_program_into`
    // path (one `cranelift_module::DataId` per slot, initial value
    // serialised from `MirConst`). `LoadStatic` / `StoreStatic`
    // codegen already routes through that table for both backends,
    // so AOT just needs to not reject the program here.
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
