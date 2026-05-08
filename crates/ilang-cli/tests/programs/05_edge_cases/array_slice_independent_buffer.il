// `arr.slice(lo, hi)` returns a fresh array — mutations to the slice
// must NOT leak into the source. The opposite must also hold: the
// source can mutate without disturbing previously-taken slices.
// `array_basics.il` checks slice contents but not the buffer-
// independence property, which is easy to regress (a slice that
// shares the underlying buffer would fail this fixture loudly).
use test

let a: i64[] = [1, 2, 3, 4, 5]
let b = a.slice(1, 4)            // [2, 3, 4]
test.expect(b.length, 3)
test.expect(b[0], 2)

b[0] = 999
test.expect(a[1], 2)             // a unchanged
test.expect(b[0], 999)

a[2] = 777
test.expect(b[1], 3)             // b unchanged

b.push(42)
test.expect(b.length, 4)
test.expect(a.length, 5)         // a length unchanged

// Slice-then-slice keeps independence too.
let c = b.slice(0, 2)
c[0] = -1
test.expect(b[0], 999)
test.expect(c[0], -1)
