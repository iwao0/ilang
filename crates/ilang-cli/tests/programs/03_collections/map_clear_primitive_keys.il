// `Map<K, V>.clear()` — drop every entry, size goes to zero, all
// previous keys answer `has(k) == false`. Per syntax.md §9 method
// table. `map_basics.il` exercises set / get / has / delete but
// not clear; this fixture covers clear() across the supported
// primitive key kinds (string / integer / bool).
use std.test as test

// String keys.
let ms: Map<string, i64> = {"a": 1, "b": 2, "c": 3}
test.expect(ms.size(), 3)
ms.clear()
test.expect(ms.size(), 0)
test.expectBool(ms.has("a"), false)
test.expectBool(ms.has("b"), false)
// After clear we can repopulate cleanly.
ms["x"] = 9
test.expect(ms.size(), 1)
test.expect(ms["x"], 9)

// Integer keys.
let mi: Map<i64, string> = {10: "ten", 20: "twenty"}
test.expect(mi.size(), 2)
mi.clear()
test.expect(mi.size(), 0)
test.expectBool(mi.has(10), false)

// Bool keys — both slots get cleared.
let mb: Map<bool, i64> = {true: 1, false: 0}
test.expect(mb.size(), 2)
mb.clear()
test.expect(mb.size(), 0)
test.expectBool(mb.has(true), false)
test.expectBool(mb.has(false), false)
