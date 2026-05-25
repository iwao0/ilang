// `.map` and `.filter` returning fresh arrays, including the empty
// cases (empty input → empty output, predicate-rejects-all,
// predicate-accepts-all) and chaining. `closures.il` exercises
// `.map` once with a captured variable; this fixture nails down
// the result-shape and chain semantics across the empty-input edge.
use std.test as test
let xs: i64[] = [1, 2, 3, 4, 5]

let squared = xs.map(fn(x: i64): i64 { x * x })
test.expect(squared.length, 5)
test.expect(squared[0], 1)
test.expect(squared[4], 25)

// map on empty input yields empty.
let empty: i64[] = []
let mapped = empty.map(fn(x: i64): i64 { x * 2 })
test.expect(mapped.length, 0)

// filter — keep evens.
let evens = xs.filter(fn(x: i64): bool { x % 2 == 0 })
test.expect(evens.length, 2)
test.expect(evens[0], 2)
test.expect(evens[1], 4)

// filter — predicate rejects all.
let none_kept = xs.filter(fn(x: i64): bool { x > 100 })
test.expect(none_kept.length, 0)

// filter — predicate accepts all.
let all_kept = xs.filter(fn(x: i64): bool { true })
test.expect(all_kept.length, 5)

// Chain: map → map → filter.
let chained = xs
    .map(fn(x: i64): i64 { x + 1 })
    .map(fn(x: i64): i64 { x * 2 })
    .filter(fn(x: i64): bool { x > 5 })
test.expect(chained.length, 4)
test.expect(chained[0], 6)         // (2+1)*2
test.expect(chained[3], 12)        // (5+1)*2

// Source not mutated by chain.
test.expect(xs.length, 5)
test.expect(xs[0], 1)
