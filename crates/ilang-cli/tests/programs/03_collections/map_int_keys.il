// Map with non-string key types (i64 / bool).
use std.test as test
let m = new Map<i64, string>()
m.set(1, "one")
m.set(2, "two")
m.set(42, "answer")
test.expect(m.size(), 3)
test.expectStr(m[1], "one")
test.expectStr(m[42], "answer")
test.expectBool(m.has(99), false)

let bm = new Map<bool, i64>()
bm.set(true, 1)
bm.set(false, 0)
test.expect(bm[true], 1)
test.expect(bm[false], 0)
test.expect(bm.size(), 2)
