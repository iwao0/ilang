// When multiple overloads are viable via implicit conversions, the
// scoring rule picks the one with the closest match. Exact type wins
// over implicit-widening, which wins over int→float.
use std.test as test
fn pick(x: i64): string { "i64" }
fn pick(x: f64): string { "f64" }

// Exact i64: takes the i64 overload.
test.expectStr(pick(42), "i64")

// Exact f64: takes the f64 overload.
test.expectStr(pick(3.14), "f64")

// i32 promotes to i64 (cost 1) vs i32 → f64 (cost 2): i64 wins.
let n: i32 = 5
test.expectStr(pick(n), "i64")
