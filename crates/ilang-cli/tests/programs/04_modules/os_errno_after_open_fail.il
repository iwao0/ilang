// `os.errno()` reflects the most recent failing libc call. We
// trigger a deterministic failure by `fopen`-ing a path that can't
// exist, then verify errno is non-zero (ENOENT = 2 on macOS /
// Linux, but we just check positivity to avoid platform variance).
//
// Native extern (`@extern("c")`) is JIT-only, so the fixture
// declares the run as JIT-only. The interpreter side of os.errno
// is exercised by `os_errno_basic.il`.
// interp: skip
use os
use test

@extern("c") class FILE {}
@extern("c") fn fopen(path: string, mode: string): FILE?

let _ = fopen("/this/path/cannot/possibly/exist__ilang_test", "r")
let e = os.errno()
test.expectTrue(e > 0)
