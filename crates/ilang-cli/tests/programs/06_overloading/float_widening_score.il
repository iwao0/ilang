// `f32 ↔ f64` widening costs 1 in overload scoring — the same as
// integer widening within a sign. A call with an `f32` argument
// against `(f32, _)` vs `(f64, _)` overloads should pick the
// exact-match `f32` variant. Per syntax.md §6 "Picking rule".
use std.test as test

fn pick(x: f32, _y: i64): string { "f32" }
fn pick(x: f64, _y: i64): string { "f64" }

// Exact-match wins for either side.
let a: f32 = 1.0
let b: f64 = 1.0
test.expectStr(pick(a, 0), "f32")
test.expectStr(pick(b, 0), "f64")

// When the only candidate that fits the literal is `f64`, the
// widening from f32 → f64 (cost 1) still resolves.
fn only_f64(x: f64): string { "f64-only" }
test.expectStr(only_f64(a), "f64-only")
