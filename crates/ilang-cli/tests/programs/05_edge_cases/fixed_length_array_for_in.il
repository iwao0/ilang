// Fixed-length array `T[N]` participates in `for-in` and the higher-
// order array methods (`.map`) the same way as a dynamic `T[]`.
// `array_basics.il` exercises the literal+index forms; this fixture
// pins down the iteration shape (the JIT lays out fixed-length
// arrays differently — they share the dynamic header but skip the
// realloc path).
use std.test as test
let xs: i64[3] = [10, 20, 30]
test.expect(xs.length, 3)

let total = 0
for x in xs { total += x }
test.expect(total, 60)

// `.map` returns a fresh dynamic array of the same length.
let factor = 2
let scaled = xs.map(fn(v: i64): i64 { v * factor })
test.expect(scaled.length, 3)
test.expect(scaled[0], 20)
test.expect(scaled[2], 60)

// Source not mutated.
test.expect(xs[0], 10)
test.expect(xs.length, 3)

// Heap-typed fixed-length array.
let names: string[2] = ["alice", "bob"]
let combined = ""
for n in names {
    combined = combined + n + ","
}
test.expectStr(combined, "alice,bob,")
test.expectStr(names[0], "alice")
test.expect(names.length, 2)
