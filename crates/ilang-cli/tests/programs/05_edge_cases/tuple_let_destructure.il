// `let (a, b) = pair` destructure pattern. The tuple is heterogeneous
// and may include heap elements; each binding must end up at the
// declared type. `tuple_basics.il` exercises tuple read via `t[N]`;
// this is the destructure-bind side.
use test

let pair: (i64, string) = (1, "hi")
let (a, b) = pair
test.expect(a, 1)
test.expectStr(b, "hi")

// Triple, all primitive.
let trio = (10, 20, 30)
let (x, y, z) = trio
test.expect(x + y + z, 60)

// Nested tuple — destructure the outer one only; inner stays a tuple.
let nested: (i64, (i64, string)) = (7, (8, "world"))
let (n, inner) = nested
test.expect(n, 7)
test.expect(inner[0], 8)
test.expectStr(inner[1], "world")

// Destructure a tuple containing a heap element.
let tup: (i64[], i64) = ([1, 2, 3], 9)
let (arr, num) = tup
test.expect(arr.length, 3)
test.expect(num, 9)
arr.push(4)
test.expect(arr.length, 4)
test.expect(tup[0].length, 4)        // alias preserved
