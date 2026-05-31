// Map<MyClass, V> with class-typed keys driven by manual
// `equals` / `hashCode`. Covers the full Map method matrix on a
// class-typed key so the Object store handles get / set / has /
// delete / size / clear / keys / values / entries / forEach
// alongside the primitive Int / Str stores.
use std.test as test

class Coord {
    pub x: i64
    pub y: i64
    pub init(x: i64, y: i64) {
        this.x = x
        this.y = y
    }
    pub equals(other: Coord): bool {
        this.x == other.x && this.y == other.y
    }
    pub hashCode(): i64 {
        this.x * 31 + this.y
    }
}

// ---- basics: set / get / has / delete / size ----
let m = new Map<Coord, string>()
test.expect(m.size(), 0)
m.set(new Coord(1, 2), "a")
m.set(new Coord(3, 4), "b")
m.set(new Coord(1, 2), "c")   // same key under value-equality — overwrites
test.expect(m.size(), 2)
test.expectStr(m[new Coord(1, 2)], "c")
test.expectStr(m[new Coord(3, 4)], "b")
test.expectBool(m.has(new Coord(1, 2)), true)
test.expectBool(m.has(new Coord(9, 9)), false)
test.expectBool(m.delete(new Coord(1, 2)), true)
test.expect(m.size(), 1)
test.expectBool(m.has(new Coord(1, 2)), false)

// ---- keys() / values() ----
let kv = new Map<Coord, i64>()
kv.set(new Coord(10, 20), 100)
kv.set(new Coord(30, 40), 200)
test.expect(kv.keys().length, 2)
test.expect(kv.values().length, 2)
let key_sum = 0
let val_sum = 0
let ks = kv.keys()
let vs = kv.values()
test.expect(ks[0].x + ks[0].y + ks[1].x + ks[1].y, 100)   // 10+20 + 30+40
test.expect(vs[0] + vs[1], 300)

// ---- entries() — list of (key, value) tuples ----
let entries = kv.entries()
test.expect(entries.length, 2)
let e_sum = 0
for e in entries {
    e_sum = e_sum + e[0].x + e[0].y + e[1]
}
test.expect(e_sum, 400)   // (10+20+100) + (30+40+200)

// ---- forEach: callback gets key + value ----
let touch_count = 0
let touch_total = 0
kv.forEach(fn(k: Coord, v: i64) {
    touch_count = touch_count + 1
    touch_total = touch_total + k.x + k.y + v
})
test.expect(touch_count, 2)
test.expect(touch_total, 400)

// ---- clear ----
kv.clear()
test.expect(kv.size(), 0)
test.expectBool(kv.has(new Coord(10, 20)), false)
