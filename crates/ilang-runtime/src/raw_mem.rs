//! Raw memory FFI read / write helpers. Mostly used by `@extern(C)`
//! call sites that need to poke specific byte sizes into a C-side
//! struct buffer.

#[unsafe(export_name = "$ffi.readI8")]
pub extern "C" fn __read_i8(p: i64, off: i64) -> i64 {
    unsafe { *((p + off) as *const i8) as i64 }
}
#[unsafe(export_name = "$ffi.readI16")]
pub extern "C" fn __read_i16(p: i64, off: i64) -> i64 {
    unsafe { *((p + off) as *const i16) as i64 }
}
#[unsafe(export_name = "$ffi.readI32")]
pub extern "C" fn __read_i32(p: i64, off: i64) -> i64 {
    unsafe { *((p + off) as *const i32) as i64 }
}
#[unsafe(export_name = "$ffi.readI64")]
pub extern "C" fn __read_i64(p: i64, off: i64) -> i64 {
    unsafe { *((p + off) as *const i64) }
}
#[unsafe(export_name = "$ffi.readU8")]
pub extern "C" fn __read_u8(p: i64, off: i64) -> i64 {
    unsafe { *((p + off) as *const u8) as i64 }
}
#[unsafe(export_name = "$ffi.readU16")]
pub extern "C" fn __read_u16(p: i64, off: i64) -> i64 {
    unsafe { *((p + off) as *const u16) as i64 }
}
#[unsafe(export_name = "$ffi.readU32")]
pub extern "C" fn __read_u32(p: i64, off: i64) -> i64 {
    unsafe { *((p + off) as *const u32) as i64 }
}
#[unsafe(export_name = "$ffi.readU64")]
pub extern "C" fn __read_u64(p: i64, off: i64) -> i64 {
    unsafe { *((p + off) as *const u64) as i64 }
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
pub extern "C" fn __write_i8(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut i8) = v as i8; }
}
#[unsafe(export_name = "$ffi.writeI16")]
pub extern "C" fn __write_i16(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut i16) = v as i16; }
}
#[unsafe(export_name = "$ffi.writeI32")]
pub extern "C" fn __write_i32(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut i32) = v as i32; }
}
#[unsafe(export_name = "$ffi.writeI64")]
pub extern "C" fn __write_i64(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut i64) = v; }
}
#[unsafe(export_name = "$ffi.writeU8")]
pub extern "C" fn __write_u8(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut u8) = v as u8; }
}
#[unsafe(export_name = "$ffi.writeU16")]
pub extern "C" fn __write_u16(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut u16) = v as u16; }
}
#[unsafe(export_name = "$ffi.writeU32")]
pub extern "C" fn __write_u32(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut u32) = v as u32; }
}
#[unsafe(export_name = "$ffi.writeU64")]
pub extern "C" fn __write_u64(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut u64) = v as u64; }
}
#[unsafe(export_name = "$ffi.writeF32")]
pub extern "C" fn __write_f32(p: i64, off: i64, v: f32) {
    unsafe { *((p + off) as *mut f32) = v; }
}
#[unsafe(export_name = "$ffi.writeF64")]
pub extern "C" fn __write_f64(p: i64, off: i64, v: f64) {
    unsafe { *((p + off) as *mut f64) = v; }
}
