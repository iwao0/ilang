// Range edge cases that `for_in_range.il` doesn't pin down: a
// "backwards" range (start > end) is empty, NOT iterating from
// start downward; an inclusive single-element range hits exactly
// once; the half-open singleton `n..n+1` also hits exactly once.
use test

let n = 0
for i in 5..3 { n += 1 }
test.expect(n, 0)

let m = 0
for i in 5..=3 { m += 1 }
test.expect(m, 0)

// Half-open singleton.
let s = 0
for i in 7..8 { s += i }
test.expect(s, 7)

// Inclusive singleton.
let one = 0
for i in 4..=4 { one += i }
test.expect(one, 4)

// Negative-spanning range works.
let total = 0
for i in -3..3 { total += i }
test.expect(total, -3)        // -3 + -2 + -1 + 0 + 1 + 2

// Inclusive negative-spanning.
let total2 = 0
for i in -2..=2 { total2 += i }
test.expect(total2, 0)
