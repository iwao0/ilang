// Tuple literal + type annotation + element access via `[N]` (where
// N is a constant integer literal). Heterogeneous element types and
// nested tuples should both work; ARC keeps heap elements alive
// across reads.
use test

// Heterogeneous primitive elements with explicit type annotation.
let p: (i64, string) = (1, "hello")
test.expect(p[0], 1)
test.expectStr(p[1], "hello")

// Inferred tuple type — bool + f64.
let q = (true, 3.14)
test.expectBool(q[0], true)
test.expectF64(q[1], 3.14)

// Nested tuple — element type is itself a tuple.
let r: (i64, (i64, string)) = (7, (8, "world"))
test.expect(r[0], 7)
test.expect(r[1][0], 8)
test.expectStr(r[1][1], "world")

// Tuple containing a heap-typed element. The tuple keeps the array
// alive; reading element 0 must observe the same array contents.
let s: (i64[], i64) = ([1, 2, 3], 9)
test.expect(s[0].length, 3)
test.expect(s[0][2], 3)
test.expect(s[1], 9)

// Tuple as a function-local binding works the same way.
let trio = (10, 20, 30)
test.expect(trio[0], 10)
test.expect(trio[1], 20)
test.expect(trio[2], 30)
