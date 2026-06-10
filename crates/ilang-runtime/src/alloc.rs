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
/// Number of `u64` slots placed before and after each user allocation
/// when `ILANG_HEAP_GUARD=1`. Filled with `GUARD_PATTERN` at alloc and
/// checked at free; any deviation aborts with the offending size +
/// surviving bytes so a buffer-overrun source can be located without
/// ASAN. Two slots (= 16 bytes each side) is enough to catch one-word
/// over/underwrites and keeps the overhead modest.
const GUARD_SLOTS: usize = 2;
const GUARD_PATTERN: u64 = 0xDEAD_BEEF_CAFE_BABE;

fn heap_guard_enabled() -> bool {
    static FLAG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FLAG.get_or_init(|| {
        std::env::var_os("ILANG_HEAP_GUARD")
            .map(|s| !s.is_empty())
            .unwrap_or(false)
    })
}

#[unsafe(export_name = "$alloc.alloc")]
pub extern "C" fn __mir_alloc(size: i64) -> i64 {
    // Back the buffer with a `Vec<u64>` so the returned address is
    // 8-byte aligned. Cranelift codegen marks heap loads/stores with
    // `MemFlags::trusted()` (= aligned hint); an unaligned i64 load on
    // aarch64 can EXC_BAD_ACCESS or, when the kernel fixes it up,
    // corrupt neighbouring heap bytes that later free paths abort on.
    // `Vec<u8>` only promised 1-byte alignment and silently produced
    // off-by-4 addresses under ASLR seeds, which is how
    // `crepr_struct_assign_index_field.il` failed ~1.5% of parallel
    // launches with empty-stderr SIGABRT / SIGSEGV.
    let n = size as usize;
    let n_u64 = n.div_ceil(8);
    let ptr = if heap_guard_enabled() {
        let total = n_u64 + GUARD_SLOTS * 2;
        let mut v: Vec<u64> = vec![GUARD_PATTERN; total];
        for slot in v.iter_mut().skip(GUARD_SLOTS).take(n_u64) {
            *slot = 0;
        }
        let user_ptr = unsafe { v.as_mut_ptr().add(GUARD_SLOTS) } as i64;
        std::mem::forget(v);
        user_ptr
    } else {
        let mut v: Vec<u64> = vec![0; n_u64];
        let p = v.as_mut_ptr() as i64;
        std::mem::forget(v);
        p
    };
    ALLOC_BYTES.fetch_add(size, Ordering::Relaxed);
    ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
    if heap_trace_enabled() {
        eprintln!("[alloc] size={size} ptr=0x{ptr:x}");
    }
    ptr
}

/// Free a previously `__mir_alloc`'d block. The caller passes the
/// original `size` so we can rebuild the matching `Vec<u64>` and drop
/// it. A null pointer or non-positive size is a no-op.
#[unsafe(export_name = "$alloc.free")]
pub extern "C" fn __mir_free(ptr: i64, size: i64) {
    if ptr == 0 || size <= 0 {
        if heap_trace_enabled() {
            eprintln!("[free:skip] ptr=0x{ptr:x} size={size}");
        }
        return;
    }
    let n = size as usize;
    let n_u64 = n.div_ceil(8);
    if heap_guard_enabled() {
        let total = n_u64 + GUARD_SLOTS * 2;
        let base = unsafe { (ptr as *mut u64).sub(GUARD_SLOTS) };
        // Validate guards before reconstructing the Vec — a corrupted
        // tail guard means someone wrote past the end of this alloc;
        // a corrupted head guard means write-before-start.
        let mut corruption: Vec<(&str, usize, u64)> = Vec::new();
        for i in 0..GUARD_SLOTS {
            let head = unsafe { *base.add(i) };
            if head != GUARD_PATTERN {
                corruption.push(("head", i, head));
            }
            let tail = unsafe { *base.add(GUARD_SLOTS + n_u64 + i) };
            if tail != GUARD_PATTERN {
                corruption.push(("tail", i, tail));
            }
        }
        if !corruption.is_empty() {
            eprintln!(
                "[guard] CORRUPTION at ptr=0x{ptr:x} size={size}: {:?}",
                corruption
            );
            use std::io::Write;
            let _ = std::io::stderr().flush();
            std::process::abort();
        }
        unsafe {
            let _ = Vec::from_raw_parts(base, total, total);
        }
    } else {
        unsafe {
            let _ = Vec::from_raw_parts(ptr as *mut u64, n_u64, n_u64);
        }
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
