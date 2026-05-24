// In nested for-loops, `break` and `continue` must affect the
// innermost loop only. A regression that hoists either to the outer
// loop is easy to write and silent — the program just runs fewer
// iterations than expected.
use std.test

let outer_iters = 0
let inner_iters = 0
for i in 0..3 {
    outer_iters += 1
    for j in 0..10 {
        if j == 2 { break }      // exits inner only
        inner_iters += 1
    }
}
test.expect(outer_iters, 3)
test.expect(inner_iters, 6)      // 0,1 per outer round × 3

// continue in inner skips one inner iteration, doesn't restart outer.
let seen: i64[] = []
for i in 0..3 {
    for j in 0..3 {
        if j == 1 { continue }
        seen.push(i * 10 + j)
    }
}
test.expect(seen.length, 6)
test.expect(seen[0], 0)
test.expect(seen[1], 2)
test.expect(seen[2], 10)
test.expect(seen[3], 12)
test.expect(seen[4], 20)
test.expect(seen[5], 22)

// Triple nesting — innermost break only.
let count = 0
for i in 0..2 {
    for j in 0..2 {
        for k in 0..5 {
            if k == 1 { break }
            count += 1
        }
    }
}
test.expect(count, 4)            // 1 inner-iter × 2 × 2
