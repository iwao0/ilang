// `if` is an expression. Forms: bare if, if/else, if/elif/.../else.
use std.test

// As a statement (Unit-typed if): no else needed.
let mut_log = 0
if true { mut_log = 1 }
test.expect(mut_log, 1)
if false { mut_log = 99 }
test.expect(mut_log, 1)

// As an expression with else.
let n = 5
let r = if n > 0 { n } else { -n }
test.expect(r, 5)
let r2 = if -7 > 0 { -7 } else { 7 }
test.expect(r2, 7)

// elif chain (NOT `else if`).
fn classify(x: i64): string {
    if x < 0 { "neg" }
    elif x == 0 { "zero" }
    elif x < 10 { "small" }
    else { "big" }
}
test.expectStr(classify(-1), "neg")
test.expectStr(classify(0), "zero")
test.expectStr(classify(5), "small")
test.expectStr(classify(42), "big")

// Nested if as expression.
fn signof(x: i64): i64 {
    if x > 0 { 1 } else { if x < 0 { -1 } else { 0 } }
}
test.expect(signof(7), 1)
test.expect(signof(-3), -1)
test.expect(signof(0), 0)

// Block-tail value as the if branch result.
let z = if true {
    let inner = 10
    inner * 2
} else {
    0
}
test.expect(z, 20)
