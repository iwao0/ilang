// `T → T?` auto-wrap costs 3 in overload scoring. An exact-`T`
// candidate (cost 0) wins decisively over a `T?` candidate; passing
// the optional explicitly picks the `T?` overload. Per syntax.md
// §6 "Picking rule (best-match scoring)".
use std.test as test

fn label(x: i64): string { "i64" }
fn label(x: i64?): string { "i64?" }

// An i64 literal matches the `i64` overload exactly — auto-wrap
// would cost 3 and lose.
test.expectStr(label(7), "i64")

// An explicit `some(...)` carries the `i64?` type and selects the
// optional overload (no widening involved on either side).
let s: i64? = some(7)
test.expectStr(label(s), "i64?")
test.expectStr(label(none), "i64?")
