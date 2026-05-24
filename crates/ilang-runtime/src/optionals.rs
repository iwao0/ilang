//! Optional cell layout: `[ value | rc | kind ]` (24 bytes), produced
//! by the codegen at `NewOptional`. `kind` is the `KIND_*` tag for
//! the inner value's cascade.

use crate::alloc::__mir_free;
use crate::cascade::release_field_by_kind;

/// Release an Optional cell. Decrements the rc at offset +8, runs
/// the inner-kind cascade based on the tag at +16, then frees the
/// 24-byte cell.
#[unsafe(export_name = "$optional.release")]
pub extern "C" fn __release_optional(opt_ptr: i64) {
    if opt_ptr == 0 {
        return;
    }
    let rc_ptr = (opt_ptr + 8) as *mut i64;
    match unsafe { crate::refcount::atomic_release(rc_ptr) } {
        Some(0) => {}
        _ => return,
    }
    let tag = unsafe { *((opt_ptr + 16) as *const i64) };
    let inner = unsafe { *(opt_ptr as *const i64) };
    release_field_by_kind(inner, tag);
    __mir_free(opt_ptr, 24);
}

#[unsafe(export_name = "$optional.retain")]
pub extern "C" fn __retain_optional(opt_ptr: i64) {
    if opt_ptr == 0 {
        return;
    }
    let rc_ptr = (opt_ptr + 8) as *mut i64;
    unsafe { crate::refcount::atomic_retain(rc_ptr) };
}
