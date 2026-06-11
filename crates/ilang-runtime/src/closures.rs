//! Closure cell layout per `MakeClosure` codegen:
//!   [ fn_addr @ 0 | rc @ 8 | capture_0 @ 16 | capture_1 @ 24 | ... ]
//!
//! Per-fn-addr capture metadata (offset + KIND_* tag for heap-shaped
//! slots) registers via `__register_closure_capture`; cell-byte-size
//! registers via `__register_closure_size`. JIT does this after
//! `finalize_definitions`; AOT does it inside `__ilang_aot_init`.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use crate::alloc::__mir_free;
use crate::cascade::release_field_by_kind;

/// `ILANG_DEBUG_CLOSURE=1` traces every closure-cell retain/release
/// to stderr. The env lookup is cached — the check on the hot path
/// is one atomic load.
pub(crate) fn debug_closure() -> bool {
    static FLAG: OnceLock<bool> = OnceLock::new();
    *FLAG.get_or_init(|| std::env::var_os("ILANG_DEBUG_CLOSURE").is_some())
}

static CLOSURE_CAPTURE_TABLE: OnceLock<RwLock<HashMap<i64, Arc<Vec<(i64, i64)>>>>> =
    OnceLock::new();

fn closure_capture_table() -> &'static RwLock<HashMap<i64, Arc<Vec<(i64, i64)>>>> {
    CLOSURE_CAPTURE_TABLE.get_or_init(|| RwLock::new(HashMap::new()))
}

#[unsafe(export_name = "$closure.registerCapture")]
pub extern "C" fn __register_closure_capture(fn_addr: i64, offset: i64, kind: i64) {
    let mut t = closure_capture_table().write().expect("closure capture table poisoned");
    let entry = t.entry(fn_addr).or_default();
    // Idempotent on (offset, kind): release builds merge identical
    // functions (ICF), so two Rust stubs with the same body — e.g.
    // `promise_resolve_stub` and `promise_race_resolve_stub` — share
    // one address, and each caller's registration lands on the same
    // key. A blind push would duplicate the entry and the release
    // cascade would over-release the captured value once per
    // duplicate (a promise died early and dropped its waiters).
    // Merged functions have identical code, so their capture layout
    // is identical too — deduping loses nothing.
    if entry.iter().any(|e| *e == (offset, kind)) {
        return;
    }
    Arc::make_mut(entry).push((offset, kind));
}

static CLOSURE_SIZE_TABLE: OnceLock<RwLock<HashMap<i64, i64>>> = OnceLock::new();

fn closure_size_table() -> &'static RwLock<HashMap<i64, i64>> {
    CLOSURE_SIZE_TABLE.get_or_init(|| RwLock::new(HashMap::new()))
}

#[unsafe(export_name = "$closure.registerSize")]
pub extern "C" fn __register_closure_size(fn_addr: i64, size: i64) {
    let mut t = closure_size_table().write().expect("closure size table poisoned");
    t.insert(fn_addr, size);
}

#[unsafe(export_name = "$closure.release")]
pub extern "C" fn __release_closure(closure_ptr: i64) {
    if closure_ptr == 0 {
        return;
    }
    let rc_ptr = (closure_ptr + 8) as *mut i64;
    if debug_closure() {
        let rc = unsafe { *rc_ptr };
        eprintln!("[rc] release {closure_ptr:#x} rc {rc}->{}", rc - 1);
    }
    match unsafe { crate::refcount::atomic_release(rc_ptr) } {
        Some(0) => {}
        _ => return,
    }
    let fn_addr = unsafe { *(closure_ptr as *const i64) };
    // Bump an Arc instead of cloning the Vec — the table is append-only
    // during registration and never mutated after startup, so this is a
    // cheap pointer copy. Released outside the lock to avoid re-entering
    // the table from nested releases.
    let entries = {
        let t = closure_capture_table().read().expect("closure capture table poisoned");
        t.get(&fn_addr).map(Arc::clone)
    };
    if let Some(entries) = entries {
        for (off, kind) in entries.iter() {
            let raw = unsafe { *((closure_ptr + *off) as *const i64) };
            release_field_by_kind(raw, *kind);
        }
    }
    let size = {
        let t = closure_size_table().read().expect("closure size table poisoned");
        t.get(&fn_addr).copied()
    };
    if let Some(size) = size {
        __mir_free(closure_ptr, size);
    }
}

#[unsafe(export_name = "$closure.retain")]
pub extern "C" fn __retain_closure(closure_ptr: i64) {
    if closure_ptr == 0 {
        return;
    }
    let rc_ptr = (closure_ptr + 8) as *mut i64;
    if debug_closure() {
        let rc = unsafe { *rc_ptr };
        eprintln!("[rc] retain  {closure_ptr:#x} rc {rc}->{}", rc + 1);
    }
    unsafe { crate::refcount::atomic_retain(rc_ptr) };
}
