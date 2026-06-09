//! Heap allocator + introspection.
//!
//! `__mir_alloc` / `__mir_free` are the canonical heap path every
//! other runtime module routes through. Live counts feed
//! `test.liveAllocBytes()` / `liveAllocCount()` for fixture-side
//! leak detection.

use std::sync::atomic::{AtomicI64, Ordering};

static ALLOC_BYTES: AtomicI64 = AtomicI64::new(0);
static FREE_BYTES: AtomicI64 = AtomicI64::new(0);
static ALLOC_COUNT: AtomicI64 = AtomicI64::new(0);
static FREE_COUNT: AtomicI64 = AtomicI64::new(0);

/// Read once and cache: did the user set `ILANG_HEAP_TRACE` to any
/// non-empty value? Used to gate the per-call `eprintln!` instruments
/// in `__mir_alloc` / `__mir_free`. Reading env vars per call would
/// dominate runtime cost; one `OnceLock` read is essentially free.
fn heap_trace_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var_os("ILANG_HEAP_TRACE")
            .map(|v| !v.is_empty())
            .unwrap_or(false)
    })
}

/// Allocate `size` zero-initialised bytes via Rust's global allocator
/// and leak the `Vec<u8>`'s data pointer. Mirrored by `__mir_free`,
/// which reconstructs the same `Vec` to drop. Tracked in the live-
/// alloc counters so `test.liveAlloc*()` can detect leaks.
#[unsafe(export_name = "$alloc.alloc")]
pub extern "C" fn __mir_alloc(size: i64) -> i64 {
    let n = size as usize;
    let mut v: Vec<u8> = vec![0; n];
    let ptr = v.as_mut_ptr() as i64;
    std::mem::forget(v);
    ALLOC_BYTES.fetch_add(size, Ordering::Relaxed);
    ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
    if heap_trace_enabled() {
        eprintln!("[alloc] size={size} ptr=0x{ptr:x}");
    }
    ptr
}

/// Free a previously `__mir_alloc`'d block. The caller passes the
/// original `size` so we can rebuild the matching `Vec<u8>` and drop
/// it. A null pointer or non-positive size is a no-op.
#[unsafe(export_name = "$alloc.free")]
pub extern "C" fn __mir_free(ptr: i64, size: i64) {
    if ptr == 0 || size <= 0 {
        if heap_trace_enabled() {
            eprintln!("[free:skip] ptr=0x{ptr:x} size={size}");
        }
        return;
    }
    unsafe {
        let _ = Vec::from_raw_parts(ptr as *mut u8, size as usize, size as usize);
    }
    FREE_BYTES.fetch_add(size, Ordering::Relaxed);
    FREE_COUNT.fetch_add(1, Ordering::Relaxed);
    if heap_trace_enabled() {
        eprintln!("[free] size={size} ptr=0x{ptr:x}");
    }
}

/// Bytes currently outstanding via `__mir_alloc`. Used by the
/// `test.liveAllocBytes()` JIT builtin to detect leaks.
pub fn live_alloc_bytes() -> i64 {
    ALLOC_BYTES.load(Ordering::Relaxed) - FREE_BYTES.load(Ordering::Relaxed)
}

/// Allocations currently outstanding via `__mir_alloc`. Used by
/// `test.liveAllocCount()`.
pub fn live_alloc_count() -> i64 {
    ALLOC_COUNT.load(Ordering::Relaxed) - FREE_COUNT.load(Ordering::Relaxed)
}
