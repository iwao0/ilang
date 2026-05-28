// An array of closures — stored, indexed, and iterated. Elements may
// capture outer state; dispatching by index or via forEach invokes
// the right one. Exercises closures as first-class array elements plus
// indirect calls on an indexed expression (`ops[i](x)`).
use std.test as test

let base = 100
let ops: (fn(i64): i64)[] = [
    fn(x: i64): i64 { x + 1 },
    fn(x: i64): i64 { x * 2 },
    fn(x: i64): i64 { x + base }    // captures `base`
]

// Direct index-and-call.
test.expect(ops[0](10), 11)
test.expect(ops[1](10), 20)
test.expect(ops[2](10), 110)

// Iterate, applying each to a seed and summing into a captured cell.
let seed = 5
let sum = 0
ops.forEach(fn(op: fn(i64): i64) { sum = sum + op(seed) })
test.expect(sum, 121)               // 6 + 10 + 105

// Pick by computed index.
let i = 1
test.expect(ops[i](21), 42)
