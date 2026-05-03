// Build a 100-element array via push, then sum it.
use test

let xs: i64[] = []
let i = 1
while i <= 100 {
    xs.push(i)
    i += 1
}
test.expect(xs.length, 100)

let total = 0
for x in xs { total += x }
test.expect(total, 5050)        // 1 + 2 + ... + 100

// Same via map + manual sum.
let doubled: i64[] = []
i = 0
while i < xs.length {
    doubled.push(xs[i] * 2)
    i += 1
}
let dtotal = 0
for x in doubled { dtotal += x }
test.expect(dtotal, 10100)

// Slice the middle 10 elements; verify boundaries.
let mid = xs.slice(45, 55)
test.expect(mid.length, 10)
test.expect(mid[0], 46)         // xs is 1-indexed (we pushed starting at 1)
test.expect(mid[9], 55)
