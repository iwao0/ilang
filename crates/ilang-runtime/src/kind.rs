//! Element-type tags shared across containers (array header +32,
//! tuple packed kinds, object field table, optional cell, enum
//! payload, map value kind, closure capture kind). Plus the `PK_*`
//! print-kind tags `format_kind_id` and the registration helpers
//! use.

/// `KIND_*` tags — runtime cascade dispatch. JIT mirrors these in
/// `compile.rs` and lowering emits them into per-cell headers.
pub const KIND_NONE: i64 = 0;
pub const KIND_OBJECT: i64 = 1;
pub const KIND_ARRAY: i64 = 2;
pub const KIND_OPTIONAL: i64 = 3;
pub const KIND_TUPLE: i64 = 4;
pub const KIND_MAP: i64 = 5;
pub const KIND_CLOSURE: i64 = 6;
pub const KIND_STR: i64 = 7;
pub const KIND_ENUM: i64 = 8;
pub const KIND_PROMISE: i64 = 9;
pub const KIND_SET: i64 = 10;
pub const KIND_WEAK: i64 = 11;


/// `PK_*` tags — print kind used by `format_kind_id` and the
/// per-field registration helpers.
pub const PK_I64_SIG: i64 = 0;
pub const PK_I64_UNS: i64 = 1;
pub const PK_I32_SIG: i64 = 2;
pub const PK_I32_UNS: i64 = 3;
pub const PK_I16_SIG: i64 = 4;
pub const PK_I16_UNS: i64 = 5;
pub const PK_I8_SIG: i64 = 6;
pub const PK_I8_UNS: i64 = 7;
pub const PK_BOOL: i64 = 8;
pub const PK_F64: i64 = 9;
pub const PK_F32: i64 = 10;
pub const PK_STR: i64 = 11;
pub const PK_OBJECT: i64 = 12;
/// Enum element / key in a `Set` / `Map` — the cell is a heap enum
/// (structural eq / hash via `__enum_structural_*`); printed via
/// `format_enum_into` (the eid lives at `ptr - 8`).
pub const PK_ENUM: i64 = 13;
pub const PK_ARRAY_I64_SIG: i64 = 100;
pub const PK_OTHER: i64 = -1;
