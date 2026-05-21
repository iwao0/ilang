//! Print-kind tags + their conversions.
//!
//! Two flavors of "kind" live next to each other:
//!
//! - `KIND_*` (heap cell tag): one i64 tag stored in every heap
//!   container's header (object field cascade, array element kind,
//!   optional payload). Drives the runtime's release / retain
//!   cascade.
//! - `PK_*` (print kind): per-value pretty-print tag used by
//!   `format_kind_id`, the map / enum / object pretty-printer
//!   entry-points, and the closure-capture print registry.
//!
//! The JIT carries the richer recursive [`PrintKind`] while
//! lowering — `print_kind_of(&MirTy)` produces it, and the two
//! `kind_tag_of_*` / `print_kind_id_*` helpers project it down to
//! the flat i64 tags the runtime registries expect.

use ilang_mir::{ClassLayout, MirTy};

// `KIND_*` — heap cell tag stored in every heap container's header.
// Used by the runtime cascade to know how to release each cell value
// at cascade time. NewArray / NewOptional codegen writes these
// into the heap header; `release_by_kind` reads back.
pub(super) const KIND_NONE: i64 = 0;
pub(super) const KIND_OBJECT: i64 = 1;
pub(super) const KIND_ARRAY: i64 = 2;
pub(super) const KIND_OPTIONAL: i64 = 3;
pub(super) const KIND_TUPLE: i64 = 4;
pub(super) const KIND_MAP: i64 = 5;
pub(super) const KIND_CLOSURE: i64 = 6;
pub(super) const KIND_STR: i64 = 7;
pub(super) const KIND_ENUM: i64 = 8;
pub(super) const KIND_PROMISE: i64 = 9;

// `PK_*` — per-value pretty-print tag.
pub(super) const PK_I64_SIG: i64 = 0;
pub(super) const PK_I64_UNS: i64 = 1;
pub(super) const PK_I32_SIG: i64 = 2;
pub(super) const PK_I32_UNS: i64 = 3;
pub(super) const PK_I16_SIG: i64 = 4;
pub(super) const PK_I16_UNS: i64 = 5;
pub(super) const PK_I8_SIG: i64 = 6;
pub(super) const PK_I8_UNS: i64 = 7;
pub(super) const PK_BOOL: i64 = 8;
pub(super) const PK_F64: i64 = 9;
pub(super) const PK_F32: i64 = 10;
pub(super) const PK_STR: i64 = 11;
pub(super) const PK_OBJECT: i64 = 12;
pub(super) const PK_ARRAY_I64_SIG: i64 = 100;
pub(super) const PK_OTHER: i64 = -1;

#[derive(Clone)]
pub(super) enum PrintKind {
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
    Optional,
    Tuple,
    Other,
}

/// Compute the cascade `KIND_*` tag for a static MirTy. Used at
/// compile time when a heap container (Array / Optional) emits its
/// header.
///
/// `@handle` structs (`MirTy::Object(cid)` where
/// `classes[cid].is_handle`) are pointer-sized opaque values with
/// no ARC header — their slot must be tagged `KIND_NONE` so the
/// release cascade leaves the raw OS handle alone instead of
/// reinterpreting it as an ilang object header (= ACCESS_VIOLATION
/// when the cascade reads a refcount out of foreign memory).
pub(super) fn kind_tag_of(ty: &MirTy, classes: &[ClassLayout]) -> i64 {
    match ty {
        MirTy::Object(cid) => {
            let layout = &classes[cid.0 as usize];
            if layout.is_handle {
                KIND_NONE
            } else {
                KIND_OBJECT
            }
        }
        MirTy::Array { .. } => KIND_ARRAY,
        MirTy::Optional(_) => KIND_OPTIONAL,
        MirTy::Tuple(_) => KIND_TUPLE,
        MirTy::Map { .. } => KIND_MAP,
        MirTy::Fn(_) => KIND_CLOSURE,
        MirTy::Str => KIND_STR,
        MirTy::Enum(_) => KIND_ENUM,
        MirTy::Promise(_) => KIND_PROMISE,
        _ => KIND_NONE,
    }
}

/// Reduce a `PrintKind` to the runtime's `KIND_*` tag the field
/// registry needs. The runtime cascade reads back the cell's own kind
/// for Optional / Array / Map / Tuple, so the top-level tag is all
/// we need to dispatch correctly.
pub(super) fn kind_tag_of_print_kind(k: &PrintKind) -> i64 {
    match k {
        PrintKind::Object => KIND_OBJECT,
        PrintKind::Array(_) => KIND_ARRAY,
        PrintKind::Optional => KIND_OPTIONAL,
        PrintKind::Tuple => KIND_TUPLE,
        PrintKind::Str => KIND_STR,
        _ => KIND_NONE,
    }
}

pub(super) fn print_kind_of(ty: &MirTy) -> PrintKind {
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
        MirTy::Optional(_) => PrintKind::Optional,
        MirTy::Tuple(_) => PrintKind::Tuple,
        _ => PrintKind::Other,
    }
}

/// Map a JIT-side `PrintKind` (rich, recursive) to the runtime's
/// flat `PK_*` cascade tag. Used when mirroring `EnumPrintInfo` into
/// `ilang-runtime`'s `__register_enum_print_variant_payload_pk`.
pub(super) fn print_kind_id_for_print_kind(k: &PrintKind) -> i64 {
    match k {
        PrintKind::I64Sig => PK_I64_SIG,
        PrintKind::I64Uns => PK_I64_UNS,
        PrintKind::I32Sig => PK_I32_SIG,
        PrintKind::I32Uns => PK_I32_UNS,
        PrintKind::I16Sig => PK_I16_SIG,
        PrintKind::I16Uns => PK_I16_UNS,
        PrintKind::I8Sig => PK_I8_SIG,
        PrintKind::I8Uns => PK_I8_UNS,
        PrintKind::Bool => PK_BOOL,
        PrintKind::F64 => PK_F64,
        PrintKind::F32 => PK_F32,
        PrintKind::Str => PK_STR,
        PrintKind::Object => PK_OBJECT,
        PrintKind::Array(inner) if matches!(**inner, PrintKind::I64Sig) => {
            PK_ARRAY_I64_SIG
        }
        _ => PK_OTHER,
    }
}

pub(super) fn print_kind_id(ty: &MirTy) -> i64 {
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

