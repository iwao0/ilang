//! Byte-offset / size constants for the heap layouts the codegen
//! emits. These mirror what the runtime treats as the canonical
//! shape — keep in sync with the comments in `ilang-runtime/src/`
//! when a layout changes.

/// Dynamic-array header (48 bytes, six i64 slots).
/// See `ilang-runtime/src/arrays.rs:1-9`.
pub(super) mod array_header {
    pub const LEN: i32 = 0;
    pub const CAP: i32 = 8;
    pub const DATA_PTR: i32 = 16;
    pub const RC: i32 = 24;
    pub const KIND_TAG: i32 = 32;
    pub const STRIDE: i32 = 40;
    pub const SIZE: i64 = 48;
}

/// Class instance header (16 bytes). Field data starts at `FIELD_BASE`.
/// See `ilang-runtime/src/classes.rs:4-7`.
pub(super) mod object_header {
    pub const CLASS_ID: i32 = 0;
    pub const RC: i32 = 8;
    /// First field offset; also the prefix size emitted by NewObject.
    pub const FIELD_BASE: i32 = 16;
}

/// Closure header. Capture slots start at +16.
/// See `ilang-runtime/src/closures.rs:1-7`.
pub(super) mod closure_header {
    pub const FN_ADDR: i32 = 0;
    pub const RC: i32 = 8;
    pub const CAPTURE_BASE: i32 = 16;
}

/// Enum value header. Tuple-payload slots start at +8.
/// See `ilang-runtime/src/enums.rs:1-6`.
pub(super) mod enum_header {
    pub const TAG: i32 = 0;
    pub const PAYLOAD_BASE: i32 = 8;
}

/// Tuple object header. Element slots start at +16; the
/// returned user pointer is `base + ELEM_BASE`.
/// See `ilang-runtime/src/tuples.rs:1-5`.
pub(super) mod tuple_header {
    pub const RC: i32 = 0;
    pub const PACKED: i32 = 8;
    pub const ELEM_BASE: i32 = 16;
}

/// Heap-allocated Optional<T> (3 i64 slots): `value | rc | kind_tag`.
pub(super) mod optional_header {
    pub const VALUE: i32 = 0;
    pub const RC: i32 = 8;
    pub const KIND_TAG: i32 = 16;
    pub const SIZE: i64 = 24;
}
