//! Raw memory FFI read / write helpers. Mostly used by `@extern(C)`
//! call sites that need to poke specific byte sizes into a C-side
//! struct buffer.
//!
//! Each helper's signature mirrors the ilang `@intrinsic` declaration
//! in `libs/std/ffi.il` exactly — narrow integer reads return the
//! actual integer width (not i64), so the cranelift ABI on either
//! side agrees byte-for-byte. Writes take the narrow value, too.

#[unsafe(export_name = "$ffi.readI8")]
pub extern "C" fn __read_i8(p: i64, off: i64) -> i8 {
    unsafe { *((p + off) as *const i8) }
}
#[unsafe(export_name = "$ffi.readI16")]
pub extern "C" fn __read_i16(p: i64, off: i64) -> i16 {
    unsafe { *((p + off) as *const i16) }
}
#[unsafe(export_name = "$ffi.readI32")]
pub extern "C" fn __read_i32(p: i64, off: i64) -> i32 {
    unsafe { *((p + off) as *const i32) }
}
#[unsafe(export_name = "$ffi.readI64")]
pub extern "C" fn __read_i64(p: i64, off: i64) -> i64 {
    unsafe { *((p + off) as *const i64) }
}
#[unsafe(export_name = "$ffi.readU8")]
pub extern "C" fn __read_u8(p: i64, off: i64) -> u8 {
    unsafe { *((p + off) as *const u8) }
}
#[unsafe(export_name = "$ffi.readU16")]
pub extern "C" fn __read_u16(p: i64, off: i64) -> u16 {
    unsafe { *((p + off) as *const u16) }
}
#[unsafe(export_name = "$ffi.readU32")]
pub extern "C" fn __read_u32(p: i64, off: i64) -> u32 {
    unsafe { *((p + off) as *const u32) }
}
#[unsafe(export_name = "$ffi.readU64")]
pub extern "C" fn __read_u64(p: i64, off: i64) -> u64 {
    unsafe { *((p + off) as *const u64) }
}
#[unsafe(export_name = "$ffi.readF32")]
pub extern "C" fn __read_f32(p: i64, off: i64) -> f32 {
    unsafe { *((p + off) as *const f32) }
}
#[unsafe(export_name = "$ffi.readF64")]
pub extern "C" fn __read_f64(p: i64, off: i64) -> f64 {
    unsafe { *((p + off) as *const f64) }
}

#[unsafe(export_name = "$ffi.writeI8")]
pub extern "C" fn __write_i8(p: i64, off: i64, v: i8) {
    unsafe { *((p + off) as *mut i8) = v; }
}
#[unsafe(export_name = "$ffi.writeI16")]
pub extern "C" fn __write_i16(p: i64, off: i64, v: i16) {
    unsafe { *((p + off) as *mut i16) = v; }
}
#[unsafe(export_name = "$ffi.writeI32")]
pub extern "C" fn __write_i32(p: i64, off: i64, v: i32) {
    unsafe { *((p + off) as *mut i32) = v; }
}
#[unsafe(export_name = "$ffi.writeI64")]
pub extern "C" fn __write_i64(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut i64) = v; }
}
#[unsafe(export_name = "$ffi.writeU8")]
pub extern "C" fn __write_u8(p: i64, off: i64, v: u8) {
    unsafe { *((p + off) as *mut u8) = v; }
}
#[unsafe(export_name = "$ffi.writeU16")]
pub extern "C" fn __write_u16(p: i64, off: i64, v: u16) {
    unsafe { *((p + off) as *mut u16) = v; }
}
#[unsafe(export_name = "$ffi.writeU32")]
pub extern "C" fn __write_u32(p: i64, off: i64, v: u32) {
    unsafe { *((p + off) as *mut u32) = v; }
}
#[unsafe(export_name = "$ffi.writeU64")]
pub extern "C" fn __write_u64(p: i64, off: i64, v: u64) {
    unsafe { *((p + off) as *mut u64) = v; }
}
#[unsafe(export_name = "$ffi.writeF32")]
pub extern "C" fn __write_f32(p: i64, off: i64, v: f32) {
    unsafe { *((p + off) as *mut f32) = v; }
}
#[unsafe(export_name = "$ffi.writeF64")]
pub extern "C" fn __write_f64(p: i64, off: i64, v: f64) {
    unsafe { *((p + off) as *mut f64) = v; }
}
