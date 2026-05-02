// match on enum — unit variants, payload variants, short variant form.
use test

// Unit variants only.
enum Color { red, green, blue }

fn pick(c: Color): i64 {
    match c {
        red { 1 }
        green { 2 }
        blue { 3 }
    }
}
test.expect(pick(Color.red), 1)
test.expect(pick(Color.green), 2)
test.expect(pick(Color.blue), 3)

// Long-form pattern with explicit enum name still works.
fn picky(c: Color): i64 {
    match c {
        Color.red { 10 }
        Color.green { 20 }
        Color.blue { 30 }
    }
}
test.expect(picky(Color.green), 20)

// Wildcard.
fn isRed(c: Color): bool {
    match c {
        red { true }
        _ { false }
    }
}
test.expectBool(isRed(Color.red), true)
test.expectBool(isRed(Color.blue), false)

// Tuple-payload variants.
enum Shape {
    point
    circle: (f64)
    rect: (f64, f64)
}

fn area(s: Shape): f64 {
    match s {
        point { 0.0 }
        circle(r) { 3.141592653589793 * r * r }
        rect(w, h) { w * h }
    }
}
test.expectF64(area(Shape.point), 0.0)
test.expectF64(area(Shape.circle(1.0)), 3.141592653589793)
test.expectF64(area(Shape.rect(3.0, 4.0)), 12.0)
