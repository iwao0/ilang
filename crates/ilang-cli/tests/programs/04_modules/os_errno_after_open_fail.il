// `os.errno()` reflects the most recent failing libc call. We
// trigger a deterministic failure by `fopen`-ing a path that can't
// exist, then verify errno is non-zero (ENOENT = 2 on macOS /
// Linux, but we just check positivity to avoid platform variance).
//
// `@extern(C) {...}` is JIT-only, so the fixture declares the run
// as JIT-only. The interpreter side of os.errno is exercised by
// `os_errno_basic.il`.
// interp: skip
use os
use test

@extern(C) {
    struct FILE {}
    @lib("c") fn fopen(path: *const char, mode: *const char): *FILE
}

let path = cstrFromString("/this/path/cannot/possibly/exist__ilang_test")
let mode = cstrFromString("r")
let _ = fopen(path, mode)
let e = os.errno()
test.expectTrue(e > 0)
