// Compound assignments (`+= -= *= /= %= &= |= ^= <<= >>=`) on
// non-trivial l-values: array index, map index, class field. The
// existing fixtures only exercise compound assignments on simple
// `let` bindings, so a regression in lvalue-aware lowering here
// would slip through.
use std.test as test
// Array index lvalue.
let xs: i64[] = [10, 20, 30]
xs[0] += 5
xs[1] -= 5
xs[2] *= 2
test.expect(xs[0], 15)
test.expect(xs[1], 15)
test.expect(xs[2], 60)

// Map index lvalue.
let m = new Map<string, i64>()
m["a"] = 10
m["a"] += 5
m["a"] *= 4
test.expect(m["a"], 60)
m["a"] /= 3
test.expect(m["a"], 20)
m["a"] %= 7
test.expect(m["a"], 6)

// Class field lvalue.
class Counter {
    n: i64
    init() { this.n = 0 }
}
let c = new Counter()
c.n += 5
c.n *= 3
c.n -= 1
test.expect(c.n, 14)
c.n /= 2
test.expect(c.n, 7)
c.n %= 4
test.expect(c.n, 3)

// Bitwise compound on field.
c.n = 0xff
c.n &= 0x0f
test.expect(c.n, 0x0f)
c.n |= 0x30
test.expect(c.n, 0x3f)
c.n ^= 0x0a
test.expect(c.n, 0x35)
