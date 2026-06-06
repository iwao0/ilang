// `arr.reverse()` returns a fresh reversed copy and leaves the
// receiver untouched. Per syntax.md §8 "Built-in array methods".
// `array_methods_chain.il` uses reverse inside a method chain;
// this fixture pins down the independence between input and output.
use std.test as test

// Integer array.
let a: i64[] = [1, 2, 3, 4]
let r = a.reverse()
// Reversed copy.
test.expect(r.length, 4)
test.expect(r[0], 4)
test.expect(r[1], 3)
test.expect(r[2], 2)
test.expect(r[3], 1)
// Original keeps its order.
test.expect(a.length, 4)
test.expect(a[0], 1)
test.expect(a[3], 4)

// Mutating one side doesn't ripple to the other.
r.push(99)
test.expect(r.length, 5)
test.expect(a.length, 4)
a[0] = 100
test.expect(r[3], 1)        // r[3] still maps to a's old [0]

// String array — the heap elements get retained, not aliased into
// the same slot.
let s: string[] = ["a", "b", "c"]
let sr = s.reverse()
test.expectStr(sr[0], "c")
test.expectStr(sr[1], "b")
test.expectStr(sr[2], "a")
test.expectStr(s[0], "a")
test.expectStr(s[2], "c")

// Empty / single-element edge cases.
let empty: i64[] = []
let re = empty.reverse()
test.expect(re.length, 0)

let one: i64[] = [42]
let ro = one.reverse()
test.expect(ro.length, 1)
test.expect(ro[0], 42)
