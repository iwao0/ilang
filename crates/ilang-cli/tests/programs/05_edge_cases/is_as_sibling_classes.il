// `is` walks the parent chain — a `D extends B` instance is an `A`
// AND a `B` AND a `D`, but NOT a sibling `C`. `as?` returns
// `some(...)` when the dynamic class is a subtype, `none` otherwise.
// `09_subtyping/match_common_ancestor.il` covers the static
// upcast direction; this fixture is the dynamic-test direction.
use std.test as test
class A {}
class B: A {}
class C: A {}
class D: B {}

let d: A = new D()
test.expectBool(d is A, true)
test.expectBool(d is B, true)          // D extends B
test.expectBool(d is C, false)         // C is a sibling, not an ancestor
test.expectBool(d is D, true)

let c: A = new C()
test.expectBool(c is A, true)
test.expectBool(c is B, false)
test.expectBool(c is C, true)

let b: A = new B()
test.expectBool(b is A, true)
test.expectBool(b is B, true)
test.expectBool(b is C, false)
test.expectBool(b is D, false)         // B is not a D

// `as?` mirrors `is`: success returns Some, failure returns None.
let ok = d as? B
test.expectBool(ok.isSome, true)

let bad = c as? B
test.expectBool(bad.isNone, true)

let upcast = d as? A
test.expectBool(upcast.isSome, true)

// Round-trip via Some.
let recovered = (d as? D).unwrap()
test.expectBool(recovered is D, true)
