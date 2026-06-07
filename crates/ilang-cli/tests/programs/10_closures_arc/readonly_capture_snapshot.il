// Read-only capture is a value snapshot — assigning to the outer
// variable AFTER the closure is built doesn't leak into the
// closure. Per syntax.md §6 "Read-only captures are value
// snapshots".
use std.test as test

let factor = 10
let scale = fn(x: i64): i64 { x * factor }
test.expect(scale(3), 30)

factor = 999
// The closure still sees the snapshot taken at build time.
test.expect(scale(3), 30)
