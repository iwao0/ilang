// `errnoCheck` flag: POSIX-style "return < 0 means failure, errno
// tells you why". The JIT branches on the C return; failure → None,
// success → Some(value). The user-facing return type is `i32?` /
// `i64?`. For the cause, the caller separately reads `os.errno()`
// (already TLS-correct via libc's `__errno_location` / `__error`).
//
// interp: skip
use test

@extern(errnoCheck) fn maybe_succeed(ok: i32): i32?
@extern(errnoCheck) fn maybe_succeed_i64(ok: i32): i64?

if let some(v) = maybe_succeed(1 as i32) {
    test.expect(v as i64, 42)
} else {
    test.fail("expected Some on ok=1")
}

if let some(v) = maybe_succeed(0 as i32) {
    test.fail("expected None on ok=0")
    test.expect(v as i64, -1)
}

// i64 path — verifies the wider boxed payload.
if let some(v) = maybe_succeed_i64(1 as i32) {
    test.expect(v, 1234567890123)
} else {
    test.fail("expected Some on i64 ok=1")
}

if let some(v) = maybe_succeed_i64(0 as i32) {
    test.fail("expected None on i64 ok=0")
    test.expect(v, -1)
}
