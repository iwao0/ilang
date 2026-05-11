// POSIX-style "return < 0 means failure, errno tells you why",
// expressed via the `errnoCheck(rc)` / `errnoCheckI64(rc)` helpers
// inside `@extern(C)`. The helper branches on `rc < 0`; failure →
// None, success → Some(rc). For the cause, the caller separately
// reads `os.errno()` (already TLS-correct via libc's
// `__errno_location` / `__error`).
//
// jit: skip
use test

@extern(C) {
    fn maybe_succeed(ok: i32): i32
    fn maybe_succeed_i64(ok: i32): i64

    fn safeMaybeSucceed(ok: i32): i32? {
        errnoCheck(maybe_succeed(ok))
    }
    fn safeMaybeSucceedI64(ok: i32): i64? {
        errnoCheckI64(maybe_succeed_i64(ok))
    }
}

if let some(v) = safeMaybeSucceed(1 as i32) {
    test.expect(v as i64, 42)
} else {
    test.fail("expected Some on ok=1")
}

if let some(v) = safeMaybeSucceed(0 as i32) {
    test.fail("expected None on ok=0")
    test.expect(v as i64, -1)
}

// i64 path — verifies the wider boxed payload.
if let some(v) = safeMaybeSucceedI64(1 as i32) {
    test.expect(v, 1234567890123)
} else {
    test.fail("expected Some on i64 ok=1")
}

if let some(v) = safeMaybeSucceedI64(0 as i32) {
    test.fail("expected None on i64 ok=0")
    test.expect(v, -1)
}
