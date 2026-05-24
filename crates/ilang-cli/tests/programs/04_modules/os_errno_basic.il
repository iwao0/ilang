// Basic `os.errno()` call — runs in both interpreter and JIT.
// We can't deterministically set errno from pure ilang code (no
// failing libc call without `@extern("c")`), so we just verify
// the function is reachable and returns an i32. Non-zero values
// can come from the test harness's prior std calls; either way
// the call must succeed.
use std.os
use std.test

// Just touching errno() should always work; reading the value
// is a no-op on errno itself, so a second call returns the same
// thing modulo unrelated process activity.
let _e: i32 = os.errno()
test.expect(1, 1)   // smoke pass
