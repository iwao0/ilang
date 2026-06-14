// Enum `==` / `!=` compare STRUCTURALLY: equal iff the variant tags
// match AND every payload compares equal. Previously only the
// discriminant tag was compared, so `circle(5) == circle(9)` was true.
// Numeric / multi-element / string payloads compare by value, nested
// enum payloads recurse, and object payloads compare by reference (the
// same rule plain `Object ==` uses). Ordering (`<` etc.) on repr enums
// stays tag-only.
//
// expect: ok
use std.test as test
enum Shape {
    circle: (i64)
    rect: (i64, i64)
    name: (string)
    point
}
enum Inner {
    a: (i64)
    b
}
enum Outer {
    wrap: (Inner)
    empty
}
class Obj {
    pub v: i64
    init(v: i64) { this.v = v }
}
enum Holds {
    obj: (Obj)
    nil
}
// numeric payload
test.expectBool(Shape.circle(5) == Shape.circle(5), true)
test.expectBool(Shape.circle(5) == Shape.circle(9), false)
test.expectBool(Shape.circle(5) != Shape.circle(9), true)
// multi-payload
test.expectBool(Shape.rect(2, 3) == Shape.rect(2, 3), true)
test.expectBool(Shape.rect(2, 3) == Shape.rect(2, 9), false)
// string payload (structural)
test.expectBool(Shape.name("hi") == Shape.name("hi"), true)
test.expectBool(Shape.name("hi") == Shape.name("yo"), false)
// different variant
test.expectBool(Shape.circle(5) == Shape.rect(5, 0), false)
// unit variant
test.expectBool(Shape.point == Shape.point, true)
test.expectBool(Shape.point == Shape.circle(0), false)
// nested enum (recursive)
test.expectBool(Outer.wrap(Inner.a(7)) == Outer.wrap(Inner.a(7)), true)
test.expectBool(Outer.wrap(Inner.a(7)) == Outer.wrap(Inner.a(8)), false)
test.expectBool(Outer.wrap(Inner.a(7)) == Outer.wrap(Inner.b), false)
// object payload: reference equality (matches Object ==)
let o = new Obj(1)
test.expectBool(Holds.obj(o) == Holds.obj(o), true)
test.expectBool(Holds.obj(new Obj(1)) == Holds.obj(new Obj(1)), false)
console.log("ok")
