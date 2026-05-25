// A closure stored in a class field, then invoked indirectly by
// re-binding to a local. Direct call syntax `(this.f)(x)` isn't
// supported by the parser yet, so the via-local pattern is the
// canonical shape. Verifies that the closure's captured environment
// survives storage in a class field and round-trips back out
// without being snapshotted.
use std.test as test
class Holder {
    f: fn(i64): i64
    init(g: fn(i64): i64) { this.f = g }
}

let factor = 7
let mul = fn(x: i64): i64 { x * factor }
let h = new Holder(mul)

let g = h.f
test.expect(g(3), 21)
test.expect(g(5), 35)

// Also: the closure captured `factor` by snapshot. Outer rebind
// after storage doesn't affect the stored closure.
let factor = 999
let g2 = h.f
test.expect(g2(2), 14)         // still uses the snapshotted 7

// Replace the field with a different closure — old one drops.
let add = fn(x: i64): i64 { x + 100 }
h.f = add
let g3 = h.f
test.expect(g3(5), 105)
