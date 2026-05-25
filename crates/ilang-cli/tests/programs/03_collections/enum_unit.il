// Unit-only enums (Phase 1 representation: bare i32 tag).
use std.test as test
enum Color { red, green, blue }

let c = Color.green
let n = match c {
    red { 1 }
    green { 2 }
    blue { 3 }
}
test.expect(n, 2)

// Enum equality via match (Color doesn't implement == directly).
fn isRed(c: Color): bool {
    match c {
        red { true }
        _ { false }
    }
}
test.expectBool(isRed(Color.red), true)
test.expectBool(isRed(Color.blue), false)

// Enum stored in a class field.
class Tagged {
    color: Color
    init(c: Color) { this.color = c }
}
let t = new Tagged(Color.blue)
let v = match t.color {
    red { 1 }
    green { 2 }
    blue { 3 }
}
test.expect(v, 3)
