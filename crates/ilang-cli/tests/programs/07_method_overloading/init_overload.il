// init() can be overloaded; `new C(...)` picks the best match.
use std.test

class Point {
    x: i64
    y: i64
    init() { this.x = 0; this.y = 0 }
    init(v: i64) { this.x = v; this.y = v }
    init(x: i64, y: i64) { this.x = x; this.y = y }
}

let p0 = new Point()
test.expect(p0.x, 0)
test.expect(p0.y, 0)

let p1 = new Point(7)
test.expect(p1.x, 7)
test.expect(p1.y, 7)

let p2 = new Point(3, 4)
test.expect(p2.x, 3)
test.expect(p2.y, 4)
