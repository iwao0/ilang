//! Closure cell layout per `MakeClosure` codegen:
//!   [ fn_addr @ 0 | rc @ 8 | capture_0 @ 16 | capture_1 @ 24 | ... ]
//!
//! Per-fn-addr capture metadata (offset + KIND_* tag for heap-shaped
//! slots) registers via `__register_closure_capture`; cell-byte-size
//! registers via `__register_closure_size`. JIT does this after
//! `finalize_definitions`; AOT does it inside `__ilang_aot_init`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::alloc::__mir_free;
use crate::cascade::release_field_by_kind;

static CLOSURE_CAPTURE_TABLE: OnceLock<Mutex<HashMap<i64, Vec<(i64, i64)>>>> =
    OnceLock::new();

fn closure_capture_table() -> &'static Mutex<HashMap<i64, Vec<(i64, i64)>>> {
    CLOSURE_CAPTURE_TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[unsafe(no_mangle)]
pub extern "C" fn __register_closure_capture(fn_addr: i64, offset: i64, kind: i64) {
    let mut t = closure_capture_table().lock().expect("closure capture table poisoned");
    t.entry(fn_addr).or_default().push((offset, kind));
}

static CLOSURE_SIZE_TABLE: OnceLock<Mutex<HashMap<i64, i64>>> = OnceLock::new();

fn closure_size_table() -> &'static Mutex<HashMap<i64, i64>> {
    CLOSURE_SIZE_TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[unsafe(no_mangle)]
pub extern "C" fn __register_closure_size(fn_addr: i64, size: i64) {
    let mut t = closure_size_table().lock().expect("closure size table poisoned");
    t.insert(fn_addr, size);
}

#[unsafe(no_mangle)]
pub extern "C" fn __release_closure(closure_ptr: i64) {
    if closure_ptr == 0 {
        return;
    }
    let rc_ptr = (closure_ptr + 8) as *mut i64;
    let rc = unsafe { *rc_ptr };
    if rc <= 0 {
        return;
    }
    let new_rc = rc - 1;
    unsafe {
        *rc_ptr = new_rc;
    }
    if new_rc != 0 {
        return;
    }
    let fn_addr = unsafe { *(closure_ptr as *const i64) };
    let entries = {
        let t = closure_capture_table().lock().expect("closure capture table poisoned");
        t.get(&fn_addr).cloned()
    };
    if let Some(entries) = entries {
        for (off, kind) in entries.iter() {
            let raw = unsafe { *((closure_ptr + *off) as *const i64) };
            release_field_by_kind(raw, *kind);
        }
    }
    let size = {
        let t = closure_size_table().lock().expect("closure size table poisoned");
        t.get(&fn_addr).copied()
    };
    if let Some(size) = size {
        __mir_free(closure_ptr, size);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __retain_closure(closure_ptr: i64) {
    if closure_ptr == 0 {
        return;
    }
    let rc_ptr = (closure_ptr + 8) as *mut i64;
    let rc = unsafe { *rc_ptr };
    if rc <= 0 {
        return;
    }
    unsafe {
        *rc_ptr = rc + 1;
    }
}
