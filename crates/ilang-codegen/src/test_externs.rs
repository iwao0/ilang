//! Host-side test-assertion FFI exposed to JITed code as `@extern fn`s.
//! Names match the qualified form produced by the loader
//! (`test.expect`, `test.expectStr`, ...). Each helper aborts with
//! exit code 2 on mismatch so the harness sees a non-zero status.

use cranelift_jit::JITBuilder;

use crate::runtime::StringRc;

extern "C" fn test_expect(actual: i64, expected: i64) {
    if actual != expected {
        eprintln!("test assertion failed: expected {expected}, got {actual}");
        std::process::exit(2);
    }
}

extern "C" fn test_expect_str(actual_ptr: i64, expected_ptr: i64) {
    let a = if actual_ptr == 0 {
        String::new()
    } else {
        unsafe { (*(actual_ptr as *const StringRc)).s.clone() }
    };
    let e = if expected_ptr == 0 {
        String::new()
    } else {
        unsafe { (*(expected_ptr as *const StringRc)).s.clone() }
    };
    if a != e {
        eprintln!("test assertion failed: expected {e:?}, got {a:?}");
        std::process::exit(2);
    }
}

extern "C" fn test_expect_bool(actual: i8, expected: i8) {
    if actual != expected {
        let a = actual != 0;
        let e = expected != 0;
        eprintln!("test assertion failed: expected {e}, got {a}");
        std::process::exit(2);
    }
}

extern "C" fn test_expect_f64(actual: f64, expected: f64) {
    if actual != expected {
        eprintln!("test assertion failed: expected {expected}, got {actual}");
        std::process::exit(2);
    }
}

extern "C" fn test_expect_true(condition: i8) {
    if condition == 0 {
        eprintln!("test assertion failed: expected true, got false");
        std::process::exit(2);
    }
}

extern "C" fn test_expect_false(condition: i8) {
    if condition != 0 {
        eprintln!("test assertion failed: expected false, got true");
        std::process::exit(2);
    }
}

/// Counter-wrapped libc::free. Each invocation bumps an atomic
/// counter so a test can observe how many times it was called.
/// Pair with `test.countedFreeCount` to check counts before/after
/// a section of code.
static FREE_COUNT: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

extern "C" fn test_counted_free(ptr: i64) {
    if ptr == 0 {
        return;
    }
    FREE_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    unsafe { libc_free_for_test(ptr) };
}

extern "C" fn test_counted_free_count() -> i32 {
    FREE_COUNT.load(std::sync::atomic::Ordering::SeqCst)
}

unsafe extern "C" {
    #[link_name = "free"]
    fn libc_free_for_test(ptr: i64);
}

/// Invoke a 2-argument i32 callback and return the result. Used by
/// the JIT-side callback round-trip test: lets us observe that an
/// ilang fn was actually called from a non-Cranelift context (the
/// Rust runtime here, simulating arbitrary native code).
extern "C" fn test_apply_i32_cb(
    cb: extern "C" fn(i64, i64) -> i32,
    a: i64,
    b: i64,
) -> i32 {
    cb(a, b)
}

extern "C" fn test_fail(msg_ptr: i64) {
    let msg = if msg_ptr == 0 {
        "<empty>".to_string()
    } else {
        unsafe { (*(msg_ptr as *const StringRc)).s.clone() }
    };
    eprintln!("test assertion failed: {msg}");
    std::process::exit(2);
}

pub(crate) fn register_test_symbols(builder: &mut JITBuilder) {
    builder.symbol("test.expect", test_expect as *const u8);
    builder.symbol("test.expectStr", test_expect_str as *const u8);
    builder.symbol("test.expectBool", test_expect_bool as *const u8);
    builder.symbol("test.expectF64", test_expect_f64 as *const u8);
    builder.symbol("test.expectTrue", test_expect_true as *const u8);
    builder.symbol("test.expectFalse", test_expect_false as *const u8);
    builder.symbol("test.fail", test_fail as *const u8);
    builder.symbol("test.applyI32Cb", test_apply_i32_cb as *const u8);
    builder.symbol("test.countedFree", test_counted_free as *const u8);
    builder.symbol("test.countedFreeCount", test_counted_free_count as *const u8);
}
