// A std.os sink with only `file` granted — the `os` capability is
// withheld, so the errno intrinsic must be refused. Confirms the
// vocabulary is enforced per-capability, not all-or-nothing.
//
// expect-error: capability 'os'
use std.os as os
os.setErrno(os.ENOENT)
console.log(os.errno().toString())
