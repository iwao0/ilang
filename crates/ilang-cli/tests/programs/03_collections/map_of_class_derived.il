// `@derive(Eq, Hash)` driving `Map<Class, V>`. Parallel of
// `set_of_class_derived.il` for maps — the synthesised `equals` /
// `hashCode` should make two distinct `new Pos(...)` instances
// index the same slot.
use std.test as test

@derive(Eq, Hash)
class Pos {
    pub r: i64
    pub c: i64
    pub init(r: i64, c: i64) {
        this.r = r
        this.c = c
    }
}

let m = new Map<Pos, i64>()
m.set(new Pos(0, 0), 100)
m.set(new Pos(1, 2), 200)
m.set(new Pos(0, 0), 999)   // structural duplicate — overwrites
test.expect(m.size(), 2)
test.expect(m[new Pos(0, 0)], 999)
test.expect(m[new Pos(1, 2)], 200)
test.expectBool(m.has(new Pos(0, 0)), true)
test.expectBool(m.has(new Pos(5, 5)), false)
test.expectBool(m.delete(new Pos(1, 2)), true)
test.expect(m.size(), 1)
