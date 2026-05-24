// A unit-return fn (`fn f() { ... }` — no `:` return annotation)
// silently discards its trailing expression value. Same ergonomics
// as a top-level expression statement: the body is evaluated for
// its side effects and the tail value is dropped.
use std.test

fn returnsUnitWithI64Tail() {
    let a: i64 = 12
    a
}

fn returnsUnitWithBoolTail() {
    let b = true
    b
}

fn returnsUnitWithStringTail() {
    let s = "scratch"
    s
}

returnsUnitWithI64Tail()
returnsUnitWithBoolTail()
returnsUnitWithStringTail()

// Explicit return-typed fns still type-check normally.
fn declaredI64(): i64 {
    let a: i64 = 7
    a
}
test.expect(declaredI64(), 7)
