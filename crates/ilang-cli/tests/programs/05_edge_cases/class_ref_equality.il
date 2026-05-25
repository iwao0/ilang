// `==` on class instances is reference identity, NOT structural.
// `class_eq_ref.il` is the dedicated fixture but doesn't combine
// with array membership / map membership / Optional unwrap, where
// the lowering may be tempted to materialize a fresh instance.
use std.test as test
class P {
    x: i64
    init(v: i64) { this.x = v }
}

let a = new P(1)
let b = new P(1)              // structurally equal, distinct ref
let c = a                     // alias

test.expectBool(a == b, false)
test.expectBool(a == c, true)
test.expectBool(a != b, true)
test.expectBool(a != c, false)

// Identity preserved across an array round-trip.
let xs: P[] = [a, b, c]
test.expectBool(xs[0] == xs[2], true)   // both point at `a`
test.expectBool(xs[0] == xs[1], false)

// Identity preserved through Optional unwrap.
let oa: P? = some(a)
test.expectBool(oa.unwrap() == a, true)
test.expectBool(oa.unwrap() == b, false)

// Re-fetching the same array slot yields the same identity.
test.expectBool(xs[0] == xs[0], true)
