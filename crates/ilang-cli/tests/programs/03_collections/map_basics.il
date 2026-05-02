// Map<K, V> — explicit constructor + set / has / size / delete plus
// the indexing forms.
use test

let m = new Map<string, i64>()
test.expect(m.size(), 0)

m.set("a", 1)
m.set("b", 2)
m.set("c", 3)
test.expect(m.size(), 3)

// has / index get.
test.expectBool(m.has("a"), true)
test.expectBool(m.has("z"), false)
test.expect(m["a"], 1)
test.expect(m["b"], 2)

// Index assignment overwrites in place.
m["a"] = 100
test.expect(m["a"], 100)
test.expect(m.size(), 3)

// delete returns true when something was removed.
test.expectBool(m.delete("a"), true)
test.expectBool(m.has("a"), false)
test.expect(m.size(), 2)
// Deleting an absent key returns false; size unchanged.
test.expectBool(m.delete("z"), false)
test.expect(m.size(), 2)
