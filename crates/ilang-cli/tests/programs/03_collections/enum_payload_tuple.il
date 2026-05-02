// Enums with positional (tuple) payloads. The pattern binds names to
// the payload fields by position.
use test

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

// Heap payload (string).
enum Msg {
    silent
    text: (string)
    pair: (string, i64)
}

fn label(m: Msg): string {
    match m {
        silent { "<silent>" }
        text(s) { s }
        pair(s, _) { s }
    }
}
test.expectStr(label(Msg.silent), "<silent>")
test.expectStr(label(Msg.text("hello")), "hello")
test.expectStr(label(Msg.pair("answer", 42)), "answer")

// Bind the int payload as well — extract it via match.
fn second(m: Msg): i64 {
    match m {
        pair(_, n) { n }
        _ { -1 }
    }
}
test.expect(second(Msg.pair("k", 99)), 99)
test.expect(second(Msg.silent), -1)
