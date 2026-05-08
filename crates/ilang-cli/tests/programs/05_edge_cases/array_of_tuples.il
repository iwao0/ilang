// Arrays whose elements are tuples — each element carries
// heterogeneous fields accessed via `t[N]` constant index. Tuple-
// element arrays are useful as records, and the iteration shape
// has to keep tuple identity through `for-in`.
use test

let pairs = [(1, "one"), (2, "two"), (3, "three")]
test.expect(pairs.length, 3)
test.expect(pairs[1][0], 2)
test.expectStr(pairs[1][1], "two")
test.expectStr(pairs[0][1], "one")

// Iterate.
let total = 0
let labels = ""
for p in pairs {
    total += p[0]
    labels = labels + p[1] + ","
}
test.expect(total, 6)
test.expectStr(labels, "one,two,three,")

// Index-assign of a tuple element on the fixed-length array.
pairs[1] = (20, "B")
test.expect(pairs[1][0], 20)
test.expectStr(pairs[1][1], "B")
test.expect(pairs[0][0], 1)        // others untouched

// Tuple element with three fields.
let triples = [(1, "a", true), (2, "b", false), (3, "c", true)]
test.expect(triples.length, 3)
test.expectBool(triples[0][2], true)
test.expectBool(triples[1][2], false)
test.expectStr(triples[2][1], "c")
