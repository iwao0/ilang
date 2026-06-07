// `fn f(x: T)` vs `fn f(x: T?)` — overload resolution picks the
// exact non-optional one for a bare `T` value, the Optional one
// for `none` / `some(x)` / a `T?` slot. Per syntax.md §6
// "Picking rule" (T → T? auto-wrap is cost=3).
use std.test as test

fn ov(x: i64): string { "i64" }
fn ov(x: i64?): string { "opt" }

// Bare i64 — exact wins (cost 0 vs 3 for auto-wrap).
test.expectStr(ov(42), "i64")
test.expectStr(ov(0), "i64")

// `none` only fits the Optional shape.
test.expectStr(ov(none), "opt")

// `some(x)` is an Optional literal — routes through the Optional
// overload.
test.expectStr(ov(some(7)), "opt")

// An already-Optional binding hands off as-is.
let s: i64? = some(99)
test.expectStr(ov(s), "opt")

// String / Optional<string> follow the same rule.
fn lab(x: string): string { "str" }
fn lab(x: string?): string { "opt-str" }
test.expectStr(lab("hi"), "str")
test.expectStr(lab(none), "opt-str")
test.expectStr(lab(some("hi")), "opt-str")
