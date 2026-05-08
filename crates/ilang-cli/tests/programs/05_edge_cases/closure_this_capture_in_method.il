// A closure built inside a method body captures `this` implicitly.
// The closure's body can call methods on `this` — including
// mutating ones — and the writes hit the original receiver. Each
// closure remembers its own `this` (the instance the method was
// called on), not whichever instance is current when the closure
// is later invoked.
use test

class Counter {
    n: i64
    init() { this.n = 0 }
    bump() { this.n = this.n + 1 }
    get(): i64 { this.n }
    bumper(): fn() { fn() { this.bump() } }
}

let c = new Counter()
let f = c.bumper()
f()
f()
f()
test.expect(c.get(), 3)

// A bumper from a different instance binds its own `this`.
let d = new Counter()
let g = d.bumper()
g()
test.expect(d.get(), 1)
test.expect(c.get(), 3)        // c untouched

// Both bumpers can interleave without interfering.
f()
g()
g()
test.expect(c.get(), 4)
test.expect(d.get(), 3)
