// Static methods on a class — callable without an instance.
use std.test as test
class Vec2 {
    x: f64
    y: f64
    init(x: f64, y: f64) { this.x = x; this.y = y }
    len(): f64 { math.sqrt(this.x * this.x + this.y * this.y) }

    static zero(): Vec2 { new Vec2(0.0, 0.0) }
    static unit_x(): Vec2 { new Vec2(1.0, 0.0) }
    static of(x: f64, y: f64): Vec2 { new Vec2(x, y) }
    static dot(a: Vec2, b: Vec2): f64 { a.x * b.x + a.y * b.y }
}
use std.math as math
let z = Vec2.zero()
test.expectF64(z.x, 0.0)
test.expectF64(z.y, 0.0)

let p = Vec2.of(3.0, 4.0)
test.expectF64(p.len(), 5.0)

let u = Vec2.unit_x()
test.expectF64(Vec2.dot(u, p), 3.0)

// Local var shadows the class name (instance lookup still works
// because we keep both).
let Vec2_alias = Vec2.of(1.0, 1.0)
test.expectF64(Vec2_alias.x, 1.0)
