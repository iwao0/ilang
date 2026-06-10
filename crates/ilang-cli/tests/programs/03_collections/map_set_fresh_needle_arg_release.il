// Fresh needle args to Map / Set lookup methods must drop their
// transient +1 after the call. The lowerer only released fresh args
// for `m.set` / `s.add`; `get` / `has` / `delete` leaked one object
// (24 bytes) or one registry string per call — `m.has(new Key(1))`
// in a loop grew without bound.

use std.test as test

class Key {
    pub id: i64
    pub init(id: i64) { this.id = id }
    pub equals(o: Key): bool { this.id == o.id }
    pub hashCode(): i64 { this.id }
}
class Box {
    n: i64
    init(x: i64) { this.n = x }
}

// ---- object keys: get / has / delete with fresh Key needles ----
let m = new Map<Key, Box>()
m.set(new Key(1), new Box(7))
let objBase = test.liveAllocBytes()
let i = 0
while i < 200 {
    test.expectBool(m.has(new Key(1)), true)
    if let some(b) = m.get(new Key(1)) { test.expect(b.n, 7) }
    test.expectBool(m.delete(new Key(9)), false)
    i = i + 1
}
let objAfter = test.liveAllocBytes()
test.expectTrue(objAfter - objBase < 1024)

// ---- object elements: Set has / delete, add+delete churn ----
let s = new Set<Key>()
let setBase = test.liveAllocBytes()
i = 0
while i < 200 {
    s.add(new Key(1))
    test.expectBool(s.has(new Key(1)), true)
    test.expectBool(s.delete(new Key(1)), true)
    i = i + 1
}
test.expect(s.size(), 0)
let setAfter = test.liveAllocBytes()
test.expectTrue(setAfter - setBase < 1024)

// ---- string needles built at runtime (registry entries) ----
let sm = new Map<string, i64>()
sm["key0"] = 1
let ss = new Set<string>()
ss.add("hello")
let strBase = test.liveStringCount()
i = 0
while i < 200 {
    test.expectBool(sm.has("key" + "0"), true)
    test.expectBool(sm.delete("nope" + `${i}`), false)
    test.expectBool(ss.has("hel" + "lo"), true)
    test.expectBool(ss.delete("miss" + `${i}`), false)
    i = i + 1
}
let strAfter = test.liveStringCount()
test.expectTrue(strAfter - strBase < 16)
