// Set<MyClass> driven by the value-equality protocol (manual
// `equals` + `hashCode` methods). Two distinct `new Point(1, 2)`
// instances are equal under the user's structural compare, so the
// set should keep only one of them.
use std.test as test

class Point {
    pub x: i64
    pub y: i64
    pub init(x: i64, y: i64) {
        this.x = x
        this.y = y
    }
    pub equals(other: Point): bool {
        this.x == other.x && this.y == other.y
    }
    pub hashCode(): i64 {
        this.x * 31 + this.y
    }
}

let s = new Set<Point>()
s.add(new Point(1, 2))
s.add(new Point(3, 4))
s.add(new Point(1, 2))   // dup under value-equality — must collapse
test.expect(s.size(), 2)
test.expectBool(s.has(new Point(1, 2)), true)
test.expectBool(s.has(new Point(9, 9)), false)
test.expectBool(s.delete(new Point(3, 4)), true)
test.expect(s.size(), 1)
