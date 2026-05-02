// Enums with named (struct-style) payloads. Patterns can bind by
// the field name (shorthand) or rename via `field: bound_name`.
use test

enum Shape {
    circle: { radius: f64 }
    rect: { width: f64, height: f64 }
}

fn area(s: Shape): f64 {
    match s {
        circle { radius: r } { 3.141592653589793 * r * r }
        rect { width: w, height: h } { w * h }
    }
}

let c = Shape.circle { radius: 2.0 }
let r = Shape.rect { width: 3.0, height: 4.0 }

test.expectF64(area(c), 3.141592653589793 * 4.0)
test.expectF64(area(r), 12.0)

// Shorthand pattern: `{ radius }` binds `radius` to the field of the
// same name.
fn radiusOf(s: Shape): f64 {
    match s {
        circle { radius } { radius }
        _ { 0.0 }
    }
}
test.expectF64(radiusOf(c), 2.0)
test.expectF64(radiusOf(r), 0.0)
