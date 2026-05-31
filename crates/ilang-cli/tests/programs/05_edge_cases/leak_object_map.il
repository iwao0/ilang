// `Map<MyClass, V>` keys must retain on `set` and release on
// `delete` / `clear` / map-drop, exactly like values. Mirror
// `leak_object_set.il` on the map side: the key is the tracked
// class, the value is `bool` (no value-side ARC needed) so any
// leak counter increment uniquely attributes to the key-side
// release path.
use std.test as test

class Counter {
    n: i64
    init() { this.n = 0 }
    bump() { this.n = this.n + 1 }
    get(): i64 { this.n }
}

class Key {
    pub id: i64
    pub parent: Counter
    init(id: i64, p: Counter) {
        this.id = id
        this.parent = p
    }
    deinit() { this.parent.bump() }
    pub equals(other: Key): bool { this.id == other.id }
    pub hashCode(): i64 { this.id }
}

// ---- set + delete churn ----
let deinits = new Counter()
let m = new Map<Key, bool>()
let i: i64 = 0
while i < 1000 {
    let k = new Key(i, deinits)
    m.set(k, true)
    let _ = m.delete(k)
    i = i + 1
}
test.expect(deinits.get(), 1000)
test.expect(m.size(), 0)

// ---- map drop releases every retained key ----
let drops = new Counter()
{
    let m2 = new Map<Key, bool>()
    let j: i64 = 0
    while j < 100 {
        m2.set(new Key(j, drops), false)
        j = j + 1
    }
    test.expect(m2.size(), 100)
}
test.expect(drops.get(), 100)

// ---- clear() drops every key ----
let cleared = new Counter()
let m3 = new Map<Key, bool>()
let kk: i64 = 0
while kk < 50 {
    m3.set(new Key(kk, cleared), true)
    kk = kk + 1
}
test.expect(m3.size(), 50)
m3.clear()
test.expect(m3.size(), 0)
test.expect(cleared.get(), 50)
