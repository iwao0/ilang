// Match must cover every case. Wildcard `_` catches the rest;
// explicit per-variant arms work too.
use test

enum Color { red, green, blue }

// Every variant covered — no wildcard needed.
fn name(c: Color): string {
    match c {
        red { "red" }
        green { "green" }
        blue { "blue" }
    }
}
test.expectStr(name(Color.red), "red")
test.expectStr(name(Color.green), "green")
test.expectStr(name(Color.blue), "blue")

// Wildcard collects everything not explicitly listed.
fn isPrimary(c: Color): bool {
    match c {
        red { true }
        blue { true }
        _ { false }
    }
}
test.expectBool(isPrimary(Color.red), true)
test.expectBool(isPrimary(Color.blue), true)
test.expectBool(isPrimary(Color.green), false)

// Match on payload with bindings AND wildcard.
enum Shape {
    point
    circle: (f64)
    rect: (f64, f64)
}
fn isCircular(s: Shape): bool {
    match s {
        circle(_) { true }
        _ { false }
    }
}
test.expectBool(isCircular(Shape.circle(1.0)), true)
test.expectBool(isCircular(Shape.point), false)
test.expectBool(isCircular(Shape.rect(1.0, 2.0)), false)
