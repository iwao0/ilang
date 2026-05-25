// `.indexOf` returns the first occurrence (or -1), `.includes`
// returns bool. Edge cases: empty array, missing element, duplicate
// elements (must hit the FIRST one), heap-typed elements (string).
// `empty_container_ops.il` partially covers the empty case but only
// for indexOf — this fixture nails down both APIs across all corners.
use std.test as test
let xs: i64[] = [10, 20, 30, 20, 40]
test.expect(xs.indexOf(20), 1)        // first occurrence
test.expect(xs.indexOf(40), 4)
test.expect(xs.indexOf(99), -1)
test.expectBool(xs.includes(30), true)
test.expectBool(xs.includes(99), false)

// Empty.
let empty: i64[] = []
test.expect(empty.indexOf(0), -1)
test.expectBool(empty.includes(0), false)

// String elements.
let ss: string[] = ["a", "b", "c", "b"]
test.expect(ss.indexOf("b"), 1)       // first occurrence, not last
test.expect(ss.indexOf("z"), -1)
test.expectBool(ss.includes("c"), true)
test.expectBool(ss.includes(""), false)

// Single-element array.
let one: i64[] = [42]
test.expect(one.indexOf(42), 0)
test.expect(one.indexOf(99), -1)
test.expectBool(one.includes(42), true)

// After mutation, indexOf reflects new contents.
xs.push(20)
test.expect(xs.indexOf(20), 1)        // still first 20, not the new tail
xs[1] = 99
test.expect(xs.indexOf(20), 3)        // first 20 is now at index 3
test.expectBool(xs.includes(99), true)
