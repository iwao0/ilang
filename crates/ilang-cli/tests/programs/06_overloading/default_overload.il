// Overloads can coexist with default arguments. The exact-arity
// match always wins over a default-filled one — `f(x)` picks the
// one-param overload, not the two-param one with a default for `y`.
use std.test as test
// One-param overload — picked for arity-1 calls.
fn label(a: i64): string { "one" }

// Two-param overload, second has a default. Picked for arity-2 calls
// AND for arity-1 calls only when no one-param overload exists.
fn label(a: i64, b: i64 = 7): string { "two" }

test.expectStr(label(5), "one")     // exact arity-1 wins
test.expectStr(label(5, 9), "two")  // exact arity-2

// When only one overload exists, default-fill happens normally.
fn solo(a: i64, b: i64 = 100): i64 { a + b }
test.expect(solo(1), 101)
test.expect(solo(1, 2), 3)
