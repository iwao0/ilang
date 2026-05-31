// Map<MyClass, V> with class-typed keys: the value-equality
// protocol (`equals` + `hashCode` on the key class) lets two
// distinct `new Coord(1, 2)` instances index the same slot.
// Mirrors `set_of_class.il`'s coverage and adds the value-side
// behaviour (get / set / has / delete / size).
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

let m = new Map<Coord, string>()
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
