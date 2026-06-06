// `let (a, _, c) = tuple` — `_` slot ignores the bound value. Per
// syntax.md §3 "Destructuring let".
use std.test as test

// Single `_` in a triple.
let (a, _, c) = (1, 2, 3)
test.expect(a, 1)
test.expect(c, 3)
test.expect(a + c, 4)

// `_` at the head / tail.
let (_, mid, _) = (10, 20, 30)
test.expect(mid, 20)

// String / heterogeneous tuple.
let (n, _, label) = (7, "ignored", "kept")
test.expect(n, 7)
test.expectStr(label, "kept")

// Multiple `_` slots are independent (no name collision).
let (_, _, _, last) = (1, 2, 3, 4)
test.expect(last, 4)

// `_` doesn't introduce a binding — referencing it should still
// resolve to the same compiler-managed sink, not a value.
let (_, only) = (99, 100)
test.expect(only, 100)
