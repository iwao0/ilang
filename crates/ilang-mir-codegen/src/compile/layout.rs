//! Byte-offset / size constants for the heap layouts the codegen
//! emits. These mirror what the runtime treats as the canonical
//! shape — keep in sync with the comments in
//! `ilang-runtime/src/arrays.rs` etc. when the layout changes.

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
