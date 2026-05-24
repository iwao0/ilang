// `o.inner.v = 99` writes to the `v` field of the object reachable
// through `o.inner` — the existing Inner instance. Replacing
// `o.inner` with a fresh Inner detaches the original (which
// continues to live as long as the outside binding holds it).
use std.test

class Inner {
    v: i64
    init(x: i64) { this.v = x }
}

class Outer {
    inner: Inner
    init(i: Inner) { this.inner = i }
}

let i = new Inner(10)
let o = new Outer(i)

test.expect(o.inner.v, 10)
test.expect(i.v, 10)

// Field write through chain mutates the SHARED Inner.
o.inner.v = 99
test.expect(o.inner.v, 99)
test.expect(i.v, 99)

// Replacing `o.inner` with a fresh Inner — `i` still alive via the
// top-level binding, unaffected by the swap.
o.inner = new Inner(7)
test.expect(o.inner.v, 7)
test.expect(i.v, 99)

// Mutating through the new chain doesn't touch `i`.
o.inner.v = 1000
test.expect(o.inner.v, 1000)
test.expect(i.v, 99)

// Three-level nesting.
class Box {
    o: Outer
    init(x: Outer) { this.o = x }
}
let b = new Box(o)
test.expect(b.o.inner.v, 1000)
b.o.inner.v = 5
test.expect(o.inner.v, 5)
test.expect(b.o.inner.v, 5)
