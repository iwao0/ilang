//! Tuple cell layout per the codegen:
//! `[ rc | packed | e0 | e1 | … ]` with `tup_ptr` pointing at the
//! first element (i.e. `base = tup_ptr - 16`). `packed` encodes
//! `arity` in the low 16 bits plus a 4-bit `KIND_*` tag per element
//! for the first 12 slots.

use crate::alloc::__mir_free;
use crate::cascade::release_field_by_kind;

#[unsafe(no_mangle)]
pub extern "C" fn __release_tuple(tup_ptr: i64) {
    if tup_ptr == 0 {
        return;
    }
    let base = tup_ptr - 16;
    let rc_ptr = base as *mut i64;
    match unsafe { crate::refcount::atomic_release(rc_ptr) } {
        Some(0) => {}
        _ => return,
    }
    let packed = unsafe { *((base + 8) as *const i64) } as u64;
    let arity = (packed & 0xFFFF) as i64;
    for i in 0..arity.min(12) {
        let kind = ((packed >> (16 + (i as u64) * 4)) & 0xF) as i64;
        if kind != 0 {
            let elem = unsafe { *((tup_ptr + i * 8) as *const i64) };
            release_field_by_kind(elem, kind);
        }
    }
    __mir_free(base, 16 + arity.max(1) * 8);
}

#[unsafe(no_mangle)]
pub extern "C" fn __retain_tuple(tup_ptr: i64) {
    if tup_ptr == 0 {
        return;
    }
    let rc_ptr = (tup_ptr - 16) as *mut i64;
    unsafe { crate::refcount::atomic_retain(rc_ptr) };
}
