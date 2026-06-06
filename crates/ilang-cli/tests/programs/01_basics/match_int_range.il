// Integer range patterns in `match`: exclusive `..`, inclusive
// `..=`, and the half-open forms `..N`, `..=N`, `N..`.
// Per syntax.md §11 "Matching on primitives".
use std.test as test

fn bucket(n: i64): string {
    return match n {
        ..0     { "neg" }
        0..10   { "small" }
        10..=99 { "tens" }
        100..   { "big" }
        _       { "?" }
    }
}

test.expectStr(bucket(-1), "neg")
test.expectStr(bucket(-1000), "neg")
test.expectStr(bucket(0), "small")
test.expectStr(bucket(5), "small")
test.expectStr(bucket(9), "small")
test.expectStr(bucket(10), "tens")
test.expectStr(bucket(50), "tens")
test.expectStr(bucket(99), "tens")
test.expectStr(bucket(100), "big")
test.expectStr(bucket(1_000_000), "big")

// `..=N` (inclusive upper, no lower).
fn small(n: i64): bool {
    return match n {
        ..=10 { true }
        _     { false }
    }
}
test.expectBool(small(10), true)
test.expectBool(small(11), false)
test.expectBool(small(-100), true)

// Signed bounds.
fn near_zero(n: i64): bool {
    return match n {
        -3..=3 { true }
        _      { false }
    }
}
test.expectBool(near_zero(-3), true)
test.expectBool(near_zero(3), true)
test.expectBool(near_zero(-4), false)
test.expectBool(near_zero(4), false)
