// Top-level `struct` / `union` (declared outside `@extern(C) { ... }`).
// Same value-type / C-layout semantics as `@extern(C) struct`, but
// the field types are restricted: no `char`, `void`, `size_t`,
// `ssize_t`, or raw pointers, transitively.
use std.test as test
struct Point {
    x: i32
    y: i32
}

pub struct Rect {
    width: i32
    height: i32
}

union Value {
    i: i64
    f: f64
}

let p = new Point()
p.x = 3
p.y = 4
test.expect(p.x as i64, 3)
test.expect(p.y as i64, 4)

let r = new Rect()
r.width = 10
r.height = 20
test.expect(r.width as i64, 10)
test.expect(r.height as i64, 20)

let v = new Value()
v.i = 42
test.expect(v.i, 42)
