// `@derive(Eq, Hash)` should synthesise `equals` / `hashCode`
// methods structurally over the class's fields, so the resulting
// behaviour matches the hand-written version in `set_of_class.il`.
use std.test as test

@derive(Eq, Hash)
class Vec3 {
    pub x: i32
    pub y: i32
    pub z: i32
    pub init(x: i32, y: i32, z: i32) {
        this.x = x
        this.y = y
        this.z = z
    }
}

let s = new Set<Vec3>()
s.add(new Vec3(1i32, 2i32, 3i32))
s.add(new Vec3(4i32, 5i32, 6i32))
s.add(new Vec3(1i32, 2i32, 3i32))   // structural duplicate
test.expect(s.size(), 2)
test.expectBool(s.has(new Vec3(1i32, 2i32, 3i32)), true)
test.expectBool(s.has(new Vec3(7i32, 8i32, 9i32)), false)
test.expectBool(s.delete(new Vec3(4i32, 5i32, 6i32)), true)
test.expect(s.size(), 1)
