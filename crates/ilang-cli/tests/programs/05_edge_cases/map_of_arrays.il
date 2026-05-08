// `Map<K, T[]>` — values are heap arrays. Index-read returns the
// shared array reference (mutations through it visible through the
// map); index-write replaces the slot's array, leaving any prior
// outside references alone.
use test

let m = new Map<string, i64[]>()
m["a"] = [1, 2, 3]
m["b"] = [10, 20]
test.expect(m["a"].length, 3)
test.expect(m["b"][0], 10)

// Mutations through the map see the same array.
m["a"].push(4)
test.expect(m["a"].length, 4)
test.expect(m["a"][3], 4)

// Take a reference, mutate via it.
let a = m["a"]
a.push(5)
test.expect(m["a"].length, 5)

// Replace the slot — `a` (outside binding) stays at its prior
// state.
m["a"] = [99]
test.expect(m["a"].length, 1)
test.expect(m["a"][0], 99)
test.expect(a.length, 5)             // outside binding unchanged

// Iterate all values.
m["c"] = [7, 8]
let total = 0
for arr in m.values() {
    for x in arr { total += x }
}
test.expect(total, 99 + 10 + 20 + 7 + 8)
