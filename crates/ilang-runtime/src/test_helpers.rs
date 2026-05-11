//! `test.*` stdlib bindings — fixture-side assertion helpers and
//! live-allocation introspection. Exposed under the dot-separated
//! module-qualified names the ilang codegen emits.

use crate::alloc::{live_alloc_bytes, live_alloc_count};
use crate::strings::{cstr_to_str, live_string_count};

#[unsafe(export_name = "test.expect")]
pub extern "C" fn test_expect(actual: i64, expected: i64) {
    if actual != expected {
        eprintln!("test assertion failed: expected {expected}, got {actual}");
        std::process::exit(2);
    }
}

#[unsafe(export_name = "test.expectStr")]
pub extern "C" fn test_expect_str(actual: i64, expected: i64) {
    let a = cstr_to_str(actual);
    let e = cstr_to_str(expected);
    if a != e {
        eprintln!("test assertion failed: expected {e:?}, got {a:?}");
        std::process::exit(2);
    }
}

#[unsafe(export_name = "test.expectBool")]
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

#[unsafe(export_name = "test.expectF64")]
pub extern "C" fn test_expect_f64(actual: f64, expected: f64) {
    if (actual - expected).abs() > 1e-9 {
        eprintln!("test assertion failed: expected {expected}, got {actual}");
        std::process::exit(2);
    }
}

#[unsafe(export_name = "test.expectTrue")]
pub extern "C" fn test_expect_true(condition: i8) {
    if condition == 0 {
        eprintln!("test assertion failed: expected true, got false");
        std::process::exit(2);
    }
}

#[unsafe(export_name = "test.expectFalse")]
pub extern "C" fn test_expect_false(condition: i8) {
    if condition != 0 {
        eprintln!("test assertion failed: expected false, got true");
        std::process::exit(2);
    }
}

#[unsafe(export_name = "test.fail")]
pub extern "C" fn test_fail(msg: i64) {
    eprintln!("test failure: {}", cstr_to_str(msg));
    std::process::exit(2);
}

#[unsafe(export_name = "test.liveAllocBytes")]
pub extern "C" fn test_live_alloc_bytes() -> i64 {
    live_alloc_bytes()
}

#[unsafe(export_name = "test.liveAllocCount")]
pub extern "C" fn test_live_alloc_count() -> i64 {
    live_alloc_count()
}

#[unsafe(export_name = "test.liveStringCount")]
pub extern "C" fn test_live_string_count() -> i64 {
    live_string_count()
}

/// `test.applyI32Cb(cb, a, b): i32` — invoke an ilang closure
/// pointer as a 2-arg i32 callback.
#[unsafe(export_name = "test.applyI32Cb")]
pub extern "C" fn test_apply_i32_cb(closure_ptr: i64, a: i64, b: i64) -> i32 {
    if closure_ptr == 0 {
        return 0;
    }
    let fn_addr = unsafe { *(closure_ptr as *const i64) };
    let f: extern "C" fn(i64, i64, i64) -> i32 =
        unsafe { std::mem::transmute(fn_addr) };
    f(a, b, closure_ptr)
}
