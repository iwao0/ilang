//! Primitive load / store at `p + offset` (offset in bytes). Used
//! by FFI bindings to peek / poke fields off opaque C structs whose
//! layout the language doesn't know.
//!
//! `host_read_*`: bytes-narrower-than-i64 values zero-extend
//! (unsigned) or sign-extend (signed) on return so callers see the
//! right value after the i64 boxing the cross-FFI call performs.
//! `host_write_*`: the value comes in as the wider host-ABI type and
//! the helper truncates as needed.

pub(super) extern "C" fn host_read_i8(p: i64, off: i64) -> i64 {
    unsafe { *((p + off) as *const i8) as i64 }
}
pub(super) extern "C" fn host_read_i16(p: i64, off: i64) -> i64 {
    unsafe { (*((p + off) as *const i16)) as i64 }
}
pub(super) extern "C" fn host_read_i32(p: i64, off: i64) -> i64 {
    unsafe { (*((p + off) as *const i32)) as i64 }
}
pub(super) extern "C" fn host_read_i64(p: i64, off: i64) -> i64 {
    unsafe { *((p + off) as *const i64) }
}
pub(super) extern "C" fn host_read_u8(p: i64, off: i64) -> i64 {
    unsafe { (*((p + off) as *const u8)) as i64 }
}
pub(super) extern "C" fn host_read_u16(p: i64, off: i64) -> i64 {
    unsafe { (*((p + off) as *const u16)) as i64 }
}
pub(super) extern "C" fn host_read_u32(p: i64, off: i64) -> i64 {
    unsafe { (*((p + off) as *const u32)) as i64 }
}
pub(super) extern "C" fn host_read_u64(p: i64, off: i64) -> i64 {
    unsafe { *((p + off) as *const u64) as i64 }
}
pub(super) extern "C" fn host_read_f32(p: i64, off: i64) -> f32 {
    unsafe { *((p + off) as *const f32) }
}
pub(super) extern "C" fn host_read_f64(p: i64, off: i64) -> f64 {
    unsafe { *((p + off) as *const f64) }
}

pub(super) extern "C" fn host_write_i8(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut i8) = v as i8; }
}
pub(super) extern "C" fn host_write_i16(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut i16) = v as i16; }
}
pub(super) extern "C" fn host_write_i32(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut i32) = v as i32; }
}
pub(super) extern "C" fn host_write_i64(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut i64) = v; }
}
pub(super) extern "C" fn host_write_u8(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut u8) = v as u8; }
}
pub(super) extern "C" fn host_write_u16(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut u16) = v as u16; }
}
pub(super) extern "C" fn host_write_u32(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut u32) = v as u32; }
}
pub(super) extern "C" fn host_write_u64(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut u64) = v as u64; }
}
pub(super) extern "C" fn host_write_f32(p: i64, off: i64, v: f32) {
    unsafe { *((p + off) as *mut f32) = v; }
}
pub(super) extern "C" fn host_write_f64(p: i64, off: i64, v: f64) {
    unsafe { *((p + off) as *mut f64) = v; }
}
