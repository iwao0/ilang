// `==` / `!=` on objects = reference (identity) equality, NOT
// structural. Mirrors Java's `==`.
use test

class Box {
    n: i64
    init(v: i64) { this.n = v }
}

let a = new Box(1)
let b = new Box(1)
let alias = a

// Same fields don't make objects equal — different allocations.
test.expectBool(a == b, false)
test.expectBool(a != b, true)

// Aliasing the same allocation IS equal.
test.expectBool(a == alias, true)
test.expectBool(a != alias, false)
