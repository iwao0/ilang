//! MirTy → Cranelift type mapping.
//!
//! M1 covers primitive scalars and a heap-pointer placeholder for
//! reference types; richer reference-type support arrives with the
//! ARC migration in a follow-up step.

use cranelift::prelude::types as ct;
use cranelift::prelude::Type as ClifType;
use ilang_mir::MirTy;

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
        | MirTy::Fn(_) => ct::I64,
        MirTy::RawPtr { .. } => ct::I64,
        MirTy::CChar => ct::I8,
        MirTy::CVoid => return None,
        MirTy::Unit => return None,
        MirTy::TypeVar(_) => return None,
    })
}
