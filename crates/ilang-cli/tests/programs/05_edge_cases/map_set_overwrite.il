// Setting the same key repeatedly must overwrite, never grow.
// `map_basics.il` checks delete + has + get/set on distinct keys but
// doesn't pin down the "set is upsert, not insert" semantics — easy
// to regress (e.g. dual-storing the value during insertion-order
// changes).
use std.test

let m = new Map<string, i64>()
m.set("a", 1)
m.set("a", 2)
m.set("a", 3)
test.expect(m.size(), 1)
test.expect(m["a"], 3)

m["a"] = 100
test.expect(m.size(), 1)
test.expect(m["a"], 100)

// Same key via index then via set.
m["b"] = 1
m.set("b", 2)
test.expect(m.size(), 2)
test.expect(m["b"], 2)

// delete then re-set: size goes 2 → 1 → 2.
test.expectBool(m.delete("a"), true)
test.expect(m.size(), 1)
m.set("a", 99)
test.expect(m.size(), 2)
test.expect(m["a"], 99)
