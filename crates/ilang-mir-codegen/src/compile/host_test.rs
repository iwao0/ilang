//! Host trampolines for the built-in `test` module — used by the
//! fixture harness to assert program state. A failed assertion
//! prints a diagnostic to stderr and exits the process with code 2
//! so the test runner sees a fail (rather than a panic the runner
//! would have to translate).

use ilang_runtime::cstr_to_str;

/// Invokes an ilang fn closure pointer as a 2-arg i32 callback.
/// The closure layout is `[fn_ptr | rc | captures...]`; we load
/// fn_ptr at offset 0 and call it with the closure as env (the
/// trailing hidden arg matches the unified ilang calling convention).
pub(super) extern "C" fn host_test_apply_i32_cb(closure_ptr: i64, a: i64, b: i64) -> i32 {
    if closure_ptr == 0 {
        return 0;
    }
    let fn_addr = unsafe { *(closure_ptr as *const i64) };
    let f: extern "C" fn(i64, i64, i64) -> i32 = unsafe { std::mem::transmute(fn_addr) };
    f(a, b, closure_ptr)
}

pub(super) extern "C" fn host_test_expect(actual: i64, expected: i64) {
    if actual != expected {
        eprintln!("test assertion failed: expected {expected}, got {actual}");
        std::process::exit(2);
    }
}

pub(super) extern "C" fn host_test_expect_str(actual: i64, expected: i64) {
    let a = cstr_to_str(actual);
    let e = cstr_to_str(expected);
    if a != e {
        eprintln!("test assertion failed: expected {e:?}, got {a:?}");
        std::process::exit(2);
    }
}

pub(super) extern "C" fn host_test_expect_bool(actual: i8, expected: i8) {
    if actual != expected {
        eprintln!(
            "test assertion failed: expected {}, got {}",
            expected != 0,
            actual != 0
        );
        std::process::exit(2);
    }
}

pub(super) extern "C" fn host_test_expect_f64(actual: f64, expected: f64) {
    if (actual - expected).abs() > 1e-9 {
        eprintln!("test assertion failed: expected {expected}, got {actual}");
        std::process::exit(2);
    }
}

pub(super) extern "C" fn host_test_expect_true(condition: i8) {
    if condition == 0 {
        eprintln!("test assertion failed: expected true, got false");
        std::process::exit(2);
    }
}

pub(super) extern "C" fn host_test_expect_false(condition: i8) {
    if condition != 0 {
        eprintln!("test assertion failed: expected false, got true");
        std::process::exit(2);
    }
}

pub(super) extern "C" fn host_test_fail(msg: i64) {
    eprintln!("test failure: {}", cstr_to_str(msg));
    std::process::exit(2);
}
