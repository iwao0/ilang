// Scoring rule cost=2 for literal-narrowing — a numeric literal
// that fits a narrower parameter type costs more than an exact
// match on the same call. Per syntax.md §6 "Picking rule".
use std.test as test

fn pick(x: i8): string { "i8" }
fn pick(x: i64): string { "i64" }

// Literal 100 fits i8 (cost=2) AND fits i64 exactly (cost=0).
// Exact wins.
test.expectStr(pick(100), "i64")
test.expectStr(pick(-1), "i64")
test.expectStr(pick(0), "i64")

// When the only overload narrows, the literal-narrow path takes it.
fn only_narrow(x: i8): string { "i8" }
test.expectStr(only_narrow(100), "i8")
test.expectStr(only_narrow(-128), "i8")
test.expectStr(only_narrow(127), "i8")

// A 16-bit fits u16 narrows; explicit `as i64` skips narrowing
// and goes through the exact-i64 path.
fn three(x: u16): string { "u16" }
fn three(x: i64): string { "i64" }
test.expectStr(three(65535), "i64")           // exact i64 wins
test.expectStr(three(65535 as u16), "u16")    // forced narrowing
