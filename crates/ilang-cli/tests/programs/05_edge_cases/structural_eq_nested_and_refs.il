// Clean-round pin for structural `==` (round 127): deep mutual recursion
// across tuple / optional / array kinds, `==` in condition positions,
// and reference-kind slots (Map) inside a tuple comparing by pointer.
// These combinations are not covered by the round-127 fixtures.
//
// expect: ok
use std.test as test
class Box {
    n: i64
    init(x: i64) { this.n = x }
}

// --- 3-level mutual recursion: tuple of optional of dynamic array ---
let x = (some([1, 2]), 3)
let y = (some([1, 2]), 3)
let z = (some([1, 9]), 3)         // inner array element differs
// A `none` inside an unannotated tuple infers as `any?`, so the tuple
// type wouldn't match `x`'s; annotate to give the absent slot the same
// type (the structural `==` itself needs identical tuple types).
let w: (i64[]?, i64) = (none, 3)
test.expectTrue(x == y)
test.expectTrue(x != z)
test.expectTrue(x != w)           // some(...) vs none

// array of tuples of optionals
let a1: ((i64, i64?))[] = [(1, some(2)), (3, none)]
let a2: ((i64, i64?))[] = [(1, some(2)), (3, none)]
let a3: ((i64, i64?))[] = [(1, some(9)), (3, none)]
test.expectTrue(a1 == a2)
test.expectTrue(a1 != a3)

// optional of array of tuples
let o1: ((i64, i64)[])? = some([(1, 2), (3, 4)])
let o2: ((i64, i64)[])? = some([(1, 2), (3, 4)])
let o3: ((i64, i64)[])? = some([(1, 2), (9, 4)])
test.expectTrue(o1 == o2)
test.expectTrue(o1 != o3)

// --- `==` in condition positions ---
let cnt = 0
if (1, 2) == (1, 2) { cnt = cnt + 1 }
while (1, 1) != (1, 1) { cnt = cnt + 100 }   // never taken
let picked = match [1, 2] == [1, 2] {
    true { 7 }
    false { 0 }
}
test.expect(cnt, 1)
test.expect(picked, 7)

// --- reference-kind slot inside a tuple compares by pointer ---
let m = new Map<string, i64>()
m["a"] = 1
let tm1 = (m, 5)
let tm2 = (m, 5)                  // same map reference
test.expectTrue(tm1 == tm2)
let m2 = new Map<string, i64>()
m2["a"] = 1
let tm3 = (m2, 5)                 // different map, value-equal contents
test.expectTrue(tm1 != tm3)      // reference, not structural

// object slot inside an optional: reference compare
let b = new Box(1)
let ob: Box? = some(b)
test.expectTrue(ob == some(b))
test.expectTrue(ob != some(new Box(1)))

console.log("ok")
