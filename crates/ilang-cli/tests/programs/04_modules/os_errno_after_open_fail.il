// `os.errno()` reflects the most recent failing libc call. We
// trigger a deterministic failure by `fopen`-ing a path that can't
// exist, then verify errno is non-zero (ENOENT = 2 on macOS /
// Linux, but we just check positivity to avoid platform variance).
//
// `@extern(C) {...}` is JIT-only, so the fixture declares the run
// as JIT-only. The interpreter side of os.errno is exercised by
// `os_errno_basic.il`.
// jit: skip
use os
use test

@extern(C) {
    struct FILE {}
    @lib("c") fn fopen(path: *const char, mode: *const char): *FILE

    // Wrap fopen so the raw `*const char` / `*FILE` values stay
    // entirely inside the block. Returns 1 on success, 0 on
    // failure — that's all the caller needs to drive errno.
    fn tryOpen(path: string, mode: string): i32 {
        let f = fopen(cstrFromString(path), cstrFromString(mode))
        if (f as i64) == 0 { 0 as i32 } else { 1 as i32 }
    }
}

let _ = tryOpen("/this/path/cannot/possibly/exist__ilang_test", "r")
let e = os.errno()
test.expectTrue(e > 0)
