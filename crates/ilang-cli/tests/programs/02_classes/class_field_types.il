// Fields of varied types — primitive, string, Object, Array, Optional.
// Verifies that field load/store works for every JIT type kind.
use std.test as test
class Inner {
    n: i64
    init(v: i64) { this.n = v }
}

class Bag {
    a: i64
    b: f64
    c: bool
    s: string
    inner: Inner
    nums: i64[]
    maybe: i64?
    init(s_: string, n: i64) {
        this.a = n
        this.b = n as f64 * 2.5
        this.c = n > 0
        this.s = s_
        this.inner = new Inner(n * 10)
        this.nums = [n, n + 1, n + 2]
        this.maybe = some(n * 100)
    }
}

let g = new Bag("hi", 4)
test.expect(g.a, 4)
test.expectF64(g.b, 10.0)
test.expectBool(g.c, true)
test.expectStr(g.s, "hi")
test.expect(g.inner.n, 40)
test.expect(g.nums.length, 3)
test.expect(g.nums[1], 5)
test.expectBool(g.maybe.isSome, true)
test.expect(g.maybe.unwrap(), 400)

// Field assignment with various types.
g.a = 99
g.s = "bye"
g.maybe = none
test.expect(g.a, 99)
test.expectStr(g.s, "bye")
test.expectBool(g.maybe.isNone, true)
