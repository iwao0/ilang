// `@derive(Eq, Hash)` now handles `f32` / `f64` fields through
// the primitive `hashCode()` (bit pattern → i64). Two `Vec3f`
// instances with identical bytes collapse in a Set; differing
// bit patterns stay distinct.
use std.test as test

@derive(Eq, Hash)
class Vec3f {
    pub x: f32
    pub y: f32
    pub z: f32
    pub init(x: f32, y: f32, z: f32) {
        this.x = x
        this.y = y
        this.z = z
    }
}

let s = new Set<Vec3f>()
s.add(new Vec3f(1.0f32, 2.0f32, 3.0f32))
s.add(new Vec3f(1.0f32, 2.0f32, 3.0f32))   // structural dup
s.add(new Vec3f(0.0f32, 0.0f32, 0.0f32))
test.expect(s.size(), 2)
test.expectBool(s.has(new Vec3f(1.0f32, 2.0f32, 3.0f32)), true)
test.expectBool(s.has(new Vec3f(9.0f32, 9.0f32, 9.0f32)), false)

// f64 sibling — same path through the primitive hash.
@derive(Eq, Hash)
class Vec3d {
    pub x: f64
    pub y: f64
    pub z: f64
    pub init(x: f64, y: f64, z: f64) {
        this.x = x
        this.y = y
        this.z = z
    }
}

let m = new Map<Vec3d, string>()
m.set(new Vec3d(1.0, 2.0, 3.0), "p")
m.set(new Vec3d(1.0, 2.0, 3.0), "q")
m.set(new Vec3d(4.0, 5.0, 6.0), "r")
test.expect(m.size(), 2)
test.expectStr(m[new Vec3d(1.0, 2.0, 3.0)], "q")
test.expectStr(m[new Vec3d(4.0, 5.0, 6.0)], "r")
