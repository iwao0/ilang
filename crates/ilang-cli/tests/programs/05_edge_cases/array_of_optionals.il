// `i64?[]` — array of Optional elements, mixed some/none. Verifies
// that the optional-discriminant survives storage and indexed
// retrieval. `optional_*.il` covers single-binding optionals; this
// is the heap-storage-of-optionals path.
use std.test

let xs: i64?[] = [some(1), none, some(3), none, some(5)]
test.expect(xs.length, 5)
test.expectBool(xs[0].isSome, true)
test.expectBool(xs[1].isNone, true)
test.expectBool(xs[2].isSome, true)
test.expectBool(xs[3].isNone, true)
test.expectBool(xs[4].isSome, true)
test.expect(xs[0].unwrap(), 1)
test.expect(xs[2].unwrap(), 3)
test.expect(xs[4].unwrap(), 5)

// Sum non-none via for-in + if-let.
let total = 0
for x in xs {
    if let some(v) = x { total += v }
}
test.expect(total, 9)

// Push some/none after the fact.
xs.push(some(7))
xs.push(none)
test.expect(xs.length, 7)
test.expectBool(xs[5].isSome, true)
test.expect(xs[5].unwrap(), 7)
test.expectBool(xs[6].isNone, true)

// Replace via index.
xs[1] = some(100)
test.expectBool(xs[1].isSome, true)
test.expect(xs[1].unwrap(), 100)
