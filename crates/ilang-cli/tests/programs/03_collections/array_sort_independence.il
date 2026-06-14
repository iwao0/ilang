// `arr.sort(cmp)` returns a fresh sorted copy and leaves the receiver
// untouched (per syntax.md §8: reverse / map / filter / sort all leave
// the receiver alone). Existing fixtures only assert the returned array
// is sorted; the mirror of `array_reverse_independence.il` — that the
// original keeps its order and the two buffers are independent — was
// unpinned. This matters because `arr.sort(cmp)` without using the
// return value looks like an in-place sort but is a no-op.
//
// expect: ok
use std.test as test

let a = [5, 3, 1, 4, 2]
let s = a.sort(fn(x: i64, y: i64): i64 { x - y })

// The returned copy is sorted ascending.
test.expect(s[0], 1)
test.expect(s[2], 3)
test.expect(s[4], 5)

// The receiver keeps its original order — sort did NOT mutate in place.
test.expect(a[0], 5)
test.expect(a[1], 3)
test.expect(a[4], 2)

// The two arrays are independent buffers: writing the copy doesn't
// reach back into the original.
s[0] = 100
test.expect(a[2], 1)

// A second sort with a descending comparator reads the still-original
// receiver and returns another independent copy.
let d = a.sort(fn(x: i64, y: i64): i64 { y - x })
test.expect(d[0], 5)
test.expect(d[4], 1)
test.expect(a[0], 5)

console.log("ok")
