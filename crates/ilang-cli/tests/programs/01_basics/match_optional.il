// match on Optional — `some(x)` / `none` / wildcard arms,
// scalar and heap payloads, value- and statement-position
// uses, and `none` arm with early `return`.
//
// expect: hi
// expect: (none)
use std.test as test
fn fromInt(o: i64?): i64 {
    match o {
        some(v) { v + 1 }
        none    { -1 }
    }
}
test.expect(fromInt(some(10)), 11)
test.expect(fromInt(none),     -1)

// Wildcard catches everything (no `none` arm needed).
fn isSome(o: i64?): bool {
    match o {
        some(_) { true }
        _       { false }
    }
}
test.expectBool(isSome(some(0)),  true)
test.expectBool(isSome(none),     false)

// Statement position — body returns ().
fn printIt(o: string?) {
    match o {
        some(s) { console.log(s) }
        none    { console.log("(none)") }
    }
}
printIt(some("hi"))
printIt(none)

// Heap payload (string). Make sure the unwrapped value
// stays alive through the arm body.
fn greet(o: string?): string {
    match o {
        some(name) { "hello, " + name }
        none       { "hello, world" }
    }
}
test.expectStr(greet(some("ilang")), "hello, ilang")
test.expectStr(greet(none),          "hello, world")

// Diverging arm — early return must not contribute to the
// join's value type.
fn requireOr(o: i64?, fallback: i64): i64 {
    let v = match o {
        some(x) { x }
        none    { return fallback }
    }
    v * 2
}
test.expect(requireOr(some(7),  0),  14)
test.expect(requireOr(none,     99), 99)
