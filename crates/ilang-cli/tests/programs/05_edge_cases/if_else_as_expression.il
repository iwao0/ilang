// `if`/`else` is an expression — it produces a value, the two arm
// types must unify, and it can nest arbitrarily deep. The basics
// fixtures only use `if` for control flow; this one pins down the
// expression-position usage.
use std.test as test
let x = if true { 10 } else { 20 }
test.expect(x, 10)

let s = if false { "a" } else { "b" }
test.expectStr(s, "b")

// Nested.
let y = if 5 > 3 { if 1 > 0 { 100 } else { 200 } } else { 300 }
test.expect(y, 100)

// Used as a function call argument.
fn double(n: i64): i64 { n * 2 }
test.expect(double(if true { 5 } else { 0 }), 10)

// Arms returning a string (heap-typed payload).
let label = if 7 % 2 == 0 { "even" } else { "odd" }
test.expectStr(label, "odd")

// elif chain as expression.
fn classify(n: i64): string {
    if n < 0 { "neg" } elif n == 0 { "zero" } elif n < 10 { "small" } else { "big" }
}
test.expectStr(classify(-1), "neg")
test.expectStr(classify(0), "zero")
test.expectStr(classify(5), "small")
test.expectStr(classify(100), "big")
