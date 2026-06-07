// Multi-parameter overload resolution — the picker sums each
// parameter's cost. `best_match.il` covers the single-parameter
// case; this one pins down the asymmetric cost combinations the
// rule produces when two positions disagree on widening.
use std.test as test

fn g(x: i64, y: i64): string { "ii" }
fn g(x: f64, y: i64): string { "fi" }
fn g(x: f64, y: f64): string { "ff" }

// All exact.
test.expectStr(g(1, 2), "ii")
test.expectStr(g(1.0, 2), "fi")
test.expectStr(g(1.0, 2.0), "ff")

// Widening on the first parameter (i32 → i64, cost 1) keeps the
// `(i64, i64)` overload — picking `(f64, i64)` would cost an extra
// int→float conversion (cost 2) on the same slot.
let n: i32 = 5
test.expectStr(g(n, 2), "ii")

// Widening on the second only.
let m: i32 = 7
test.expectStr(g(1.0, m), "fi")

// Both positions widen (i32 → f64 on first, i32 → i64 on second).
let a: i32 = 3
let b: i32 = 4
test.expectStr(g(a, b), "ii")    // i64+i64 cheaper than f64+i64

// 3-param shapes pick from the same scoring across all slots.
fn h(x: i64, y: i64, z: i64): string { "iii" }
fn h(x: i64, y: f64, z: i64): string { "ifi" }
test.expectStr(h(1, 2, 3), "iii")
test.expectStr(h(1, 2.0, 3), "ifi")
