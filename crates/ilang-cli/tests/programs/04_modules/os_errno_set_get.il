// Round-trip `os.setErrno` then `os.errno()`, plus exercise the
// named POSIX constants. Runs in both interpreter and JIT — the
// errno store is per-thread and both backends use the same C
// runtime for the read/write.
use std.os
use std.test

os.setErrno(os.ENOENT)
test.expect(os.errno() as i64, 2)

os.setErrno(os.EACCES)
test.expect(os.errno() as i64, 13)

// Clear then check.
os.setErrno(0)
test.expect(os.errno() as i64, 0)

// Comparing errno() to a constant.
os.setErrno(os.EINVAL)
test.expectTrue(os.errno() == os.EINVAL)
