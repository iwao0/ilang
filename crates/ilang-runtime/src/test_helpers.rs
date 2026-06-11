//! `test.*` stdlib bindings — fixture-side assertion helpers and
//! live-allocation introspection. Exposed under the dot-separated
//! module-qualified names the ilang codegen emits.

use crate::alloc::{live_alloc_bytes, live_alloc_count};
use crate::strings::{cstr_to_str, live_string_count};

#[unsafe(export_name = "$test.expect")]
pub extern "C" fn test_expect(actual: i64, expected: i64) {
    if actual != expected {
        eprintln!("test assertion failed: expected {expected}, got {actual}");
        std::process::exit(2);
    }
}

#[unsafe(export_name = "$test.expectStr")]
pub extern "C" fn test_expect_str(actual: i64, expected: i64) {
    let a = cstr_to_str(actual);
    let e = cstr_to_str(expected);
    if a != e {
        eprintln!("test assertion failed: expected {e:?}, got {a:?}");
        std::process::exit(2);
    }
}

#[unsafe(export_name = "$test.expectBool")]
pub extern "C" fn test_expect_bool(actual: i8, expected: i8) {
    if actual != expected {
        eprintln!(
            "test assertion failed: expected {}, got {}",
            expected != 0,
            actual != 0
        );
        std::process::exit(2);
    }
}

#[unsafe(export_name = "$test.expectF64")]
pub extern "C" fn test_expect_f64(actual: f64, expected: f64) {
    if (actual - expected).abs() > 1e-9 {
        eprintln!("test assertion failed: expected {expected}, got {actual}");
        std::process::exit(2);
    }
}

#[unsafe(export_name = "$test.expectTrue")]
pub extern "C" fn test_expect_true(condition: i8) {
    if condition == 0 {
        eprintln!("test assertion failed: expected true, got false");
        std::process::exit(2);
    }
}

#[unsafe(export_name = "$test.expectFalse")]
pub extern "C" fn test_expect_false(condition: i8) {
    if condition != 0 {
        eprintln!("test assertion failed: expected false, got true");
        std::process::exit(2);
    }
}

#[unsafe(export_name = "$test.fail")]
pub extern "C" fn test_fail(msg: i64) {
    eprintln!("test failure: {}", cstr_to_str(msg));
    std::process::exit(2);
}

#[unsafe(export_name = "$test.liveAllocBytes")]
pub extern "C" fn test_live_alloc_bytes() -> i64 {
    // Pump the event loop first: async / Promise fixtures queue
    // continuations there even on a synchronously-resolved `.then`,
    // and an unpumped queue leaves callback-pending allocs visible
    // to the leak counter as if they were leaks. `pump` (not
    // `drain`) on purpose — a probe must not sleep until a pending
    // timer's due time, and with an armed interval a blocking drain
    // would never return. No-op when nothing is queued.
    crate::pool::pump();
    live_alloc_bytes()
}

#[unsafe(export_name = "$test.liveAllocCount")]
pub extern "C" fn test_live_alloc_count() -> i64 {
    crate::pool::pump();
    live_alloc_count()
}

#[unsafe(export_name = "$test.liveStringCount")]
pub extern "C" fn test_live_string_count() -> i64 {
    live_string_count()
}

// --------------------------------------------------------------------
// `test.mallocBytesInUse()` — process-wide heap bytes-in-use as
// reported by libmalloc's default zone. Catches leaks the ilang-side
// `liveAllocBytes` tracker misses (objc_autorelease'd ObjC blocks,
// `[NSString stringWithUTF8String:]` autoreleased temporaries, etc.).
//
// Usage in a leak fixture:
//   let body = fn() { /* workload */ }
//   autoreleasepool(body)          // warm: lazy class init etc
//   let base = test.mallocBytesInUse()
//   loop N {
//       autoreleasepool(body)
//   }
//   let delta = test.mallocBytesInUse() - base
//   // delta should be bounded (small constant) for a leak-free path
//
// macOS only — returns 0 elsewhere.
// --------------------------------------------------------------------

#[cfg(target_os = "macos")]
#[repr(C)]
struct MallocStatistics {
    blocks_in_use: u32,
    size_in_use: usize,
    max_size_in_use: usize,
    size_allocated: usize,
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn malloc_zone_statistics(zone: *mut std::ffi::c_void, stats: *mut MallocStatistics);
}

#[unsafe(export_name = "$test.mallocBytesInUse")]
pub extern "C" fn test_malloc_bytes_in_use() -> i64 {
    #[cfg(target_os = "macos")]
    {
        let mut s = MallocStatistics {
            blocks_in_use: 0,
            size_in_use: 0,
            max_size_in_use: 0,
            size_allocated: 0,
        };
        // Passing NULL as the zone tells libmalloc to aggregate
        // across every registered zone — gives a process-wide
        // bytes-in-use count, which is what we want for catching
        // ObjC blocks / autorelease'd NSStrings / etc.
        unsafe {
            malloc_zone_statistics(std::ptr::null_mut(), &mut s);
        }
        s.size_in_use as i64
    }
    #[cfg(not(target_os = "macos"))]
    {
        0
    }
}

// --------------------------------------------------------------------
// `test.countedFree` — libc::free wrapped with a counter so fixtures
// can assert how many times a custom deallocator ran. Used by FFI
// tests that ship their own `extern free_with(...)`-style helpers.
// --------------------------------------------------------------------

static COUNTED_FREE_COUNT: std::sync::atomic::AtomicI32 =
    std::sync::atomic::AtomicI32::new(0);

unsafe extern "C" {
    #[link_name = "free"]
    fn libc_free_for_test(ptr: i64);
}

#[unsafe(export_name = "$test.countedFree")]
pub extern "C" fn test_counted_free(ptr: i64) {
    if ptr == 0 {
        return;
    }
    COUNTED_FREE_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    unsafe { libc_free_for_test(ptr) };
}

#[unsafe(export_name = "$test.countedFreeCount")]
pub extern "C" fn test_counted_free_count() -> i32 {
    COUNTED_FREE_COUNT.load(std::sync::atomic::Ordering::SeqCst)
}

/// `test.applyI32Cb(cb, a, b): i32` — invoke a top-level ilang fn
/// passed across the C boundary as a callback. `@extern(C)` /
/// `@intrinsic` parameters of `fn(...)` type receive a bare
/// `FuncAddr` (8-byte code pointer, no closure box), so `cb_ptr` is
/// the function's entry address directly — not a closure header.
/// The function's cranelift signature still appends a hidden env
/// slot for non-extern fns, hence the 3-arg cast; we pass `0` for
/// env because top-level fns don't read it.
#[unsafe(export_name = "$test.applyI32Cb")]
pub extern "C" fn test_apply_i32_cb(cb_ptr: i64, a: i64, b: i64) -> i32 {
    if cb_ptr == 0 {
        return 0;
    }
    let f: extern "C" fn(i64, i64, i64) -> i32 =
        unsafe { std::mem::transmute(cb_ptr) };
    f(a, b, 0)
}
