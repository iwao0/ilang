/// NSUUID — generate + string round-trip + equality.

use std.test as test
use foundation { NSUUID, nsstr, ilstr }

let u1 = NSUUID.UUID()
let s = u1.UUIDString
// Standard UUID string is 36 chars (8-4-4-4-12 with 4 hyphens).
test.expect(s.length as i64, 36)

// Round-trip the string back into an NSUUID.
let u2 = NSUUID.alloc().initWithUUIDString(s)
test.expectStr(ilstr(u2.UUIDString), ilstr(s))

// Two fresh UUIDs differ.
let u3 = NSUUID.UUID()
test.expectFalse(u1.UUIDString.isEqualToString(u3.UUIDString))

0
