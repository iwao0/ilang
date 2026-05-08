// `m.keys()` / `m.values()` produce arrays usable in `for-in`. Each
// returned array must be sized to `m.size()` and contain (in some
// order) the live keys / values. The order is unspecified — sums and
// existence checks are the safe assertions.
use test

let m = new Map<string, i64>()
m["a"] = 1
m["b"] = 2
m["c"] = 3

test.expect(m.keys().length, 3)
test.expect(m.values().length, 3)

// Iterate keys, look each one up; value sum must equal 1+2+3.
let total_via_keys = 0
for k in m.keys() {
    total_via_keys += m[k]
}
test.expect(total_via_keys, 6)

// Iterate values directly.
let total_via_values = 0
for v in m.values() {
    total_via_values += v
}
test.expect(total_via_values, 6)

// After a delete, the snapshot's length tracks current size.
m.delete("b")
test.expect(m.keys().length, 2)
test.expect(m.values().length, 2)

let after = 0
for v in m.values() { after += v }
test.expect(after, 4)

// Re-add.
m["d"] = 10
test.expect(m.keys().length, 3)
let after2 = 0
for v in m.values() { after2 += v }
test.expect(after2, 14)
