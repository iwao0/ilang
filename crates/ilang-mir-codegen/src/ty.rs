//! MirTy → Cranelift type mapping.
//!
//! M1 covers primitive scalars and a heap-pointer placeholder for
//! reference types; richer reference-type support arrives with the
//! ARC migration in a follow-up step.

use cranelift::prelude::types as ct;
use cranelift::prelude::Type as ClifType;
use ilang_mir::{MirTy, types::SimdElem};

/// Map a MIR type to its Cranelift representation. Heap-typed values
/// land as a pointer-sized integer (`I64` on 64-bit targets); the
/// MIR consumer keeps separate per-class layouts and field offsets.
pub fn mir_to_clif(t: &MirTy) -> Option<ClifType> {
    Some(match t {
        MirTy::I8 | MirTy::U8 => ct::I8,
        MirTy::I16 | MirTy::U16 => ct::I16,
        MirTy::I32 | MirTy::U32 => ct::I32,
        MirTy::I64 | MirTy::U64 | MirTy::Size | MirTy::SSize => ct::I64,
        MirTy::F32 => ct::F32,
        MirTy::F64 => ct::F64,
        // bool widens to I8 in clif (Cranelift dropped the dedicated B1
        // type; user code chooses I8 / I32 to suit ABIs).
        MirTy::Bool => ct::I8,
        // Heap types become an i64 pointer.
        MirTy::Str
        | MirTy::Object(_)
        | MirTy::Weak(_)
        | MirTy::Enum(_)
        | MirTy::Array { .. }
        | MirTy::Tuple(_)
        | MirTy::Optional(_)
        | MirTy::Map { .. }
        | MirTy::Set { .. }
        | MirTy::Promise(_)
        | MirTy::Fn(_) => ct::I64,
        // Raw fn ptr is a bare 8-byte code address.
        MirTy::RawFn(_) => ct::I64,
        MirTy::RawPtr { .. } => ct::I64,
        MirTy::CChar => ct::I8,
        MirTy::CVoid => return None,
        MirTy::Unit => return None,
        MirTy::TypeVar(_) => return None,
        MirTy::Simd { elem, lanes } => simd_to_clif(*elem, *lanes)?,
    })
}

/// Map a `(lane_elem, lane_count)` pair to the matching cranelift
/// vector type. Returns `None` for combos cranelift doesn't carry
/// a fixed-width type for (very wide or odd lane counts) — callers
/// can fall back to memory-passing if they need to support those.
fn simd_to_clif(elem: SimdElem, lanes: u32) -> Option<ClifType> {
    Some(match (elem, lanes) {
        (SimdElem::F32, 2) => ct::F32X2,
        (SimdElem::F32, 4) => ct::F32X4,
        (SimdElem::F32, 8) => ct::F32X8,
        (SimdElem::F32, 16) => ct::F32X16,
        (SimdElem::F64, 2) => ct::F64X2,
        (SimdElem::F64, 4) => ct::F64X4,
        (SimdElem::F64, 8) => ct::F64X8,
        (SimdElem::I8, 8) => ct::I8X8,
        (SimdElem::I8, 16) => ct::I8X16,
        (SimdElem::I8, 32) => ct::I8X32,
        (SimdElem::I8, 64) => ct::I8X64,
        (SimdElem::I16, 4) => ct::I16X4,
        (SimdElem::I16, 8) => ct::I16X8,
        (SimdElem::I16, 16) => ct::I16X16,
        (SimdElem::I16, 32) => ct::I16X32,
        (SimdElem::I32, 2) => ct::I32X2,
        (SimdElem::I32, 4) => ct::I32X4,
        (SimdElem::I32, 8) => ct::I32X8,
        (SimdElem::I32, 16) => ct::I32X16,
        (SimdElem::I64, 2) => ct::I64X2,
        (SimdElem::I64, 4) => ct::I64X4,
        (SimdElem::I64, 8) => ct::I64X8,
        _ => return None,
    })
}
