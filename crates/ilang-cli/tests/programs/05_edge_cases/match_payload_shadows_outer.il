// A pattern binding inside a match arm introduces a fresh local
// scoped to that arm. It must shadow any outer variable of the same
// name without overwriting it.
use std.test

enum E {
    a: (i64)
    b: (string)
    none
}

let v = 100
let s = "outer"

let n: i64 = match E.a(7) {
    a(v) { v * 2 }              // `v` shadows outer
    b(_) { 0 }
    none { -1 }
}
test.expect(n, 14)
test.expect(v, 100)             // outer untouched

let t: string = match E.b("inner") {
    a(_) { "" }
    b(s) { s + "!" }            // `s` shadows outer
    none { "" }
}
test.expectStr(t, "inner!")
test.expectStr(s, "outer")
