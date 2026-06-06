// `f32` / `f64` associated constants + `.isNaN()` / `.isFinite()`
// predicates, per syntax.md §3.
use std.test as test

// NaN compares unequal to itself, and `.isNaN()` returns true.
test.expectBool(f32.NaN == f32.NaN, false)
test.expectBool(f64.NaN == f64.NaN, false)
test.expectBool((f32.NaN).isNaN(), true)
test.expectBool((f64.NaN).isNaN(), true)
test.expectBool((1.0_f32).isNaN(), false)
test.expectBool((1.0).isNaN(), false)

// Infinity / NegInfinity are non-finite but not NaN.
test.expectBool((f32.Infinity).isFinite(), false)
test.expectBool((f32.Infinity).isNaN(), false)
test.expectBool((f64.Infinity).isFinite(), false)
test.expectBool((f64.NegInfinity).isFinite(), false)
test.expectBool(f64.Infinity > 0.0, true)
test.expectBool(f64.NegInfinity < 0.0, true)
test.expectBool(f32.Infinity + f32.NegInfinity == f32.Infinity + f32.NegInfinity, false)

// Min is the most negative finite value (Rust-style), Max the
// largest positive. Both are finite.
test.expectBool((f32.Min).isFinite(), true)
test.expectBool((f32.Max).isFinite(), true)
test.expectBool((f64.Min).isFinite(), true)
test.expectBool((f64.Max).isFinite(), true)
test.expectBool(f32.Min < 0.0, true)
test.expectBool(f32.Max > 0.0, true)
test.expectBool(f64.Min < f64.Max, true)
test.expectF64(f64.Min, -1.7976931348623157e308)
test.expectF64(f64.Max, 1.7976931348623157e308)

// MinPositive is the smallest positive normal — positive and finite.
test.expectBool(f32.MinPositive > 0.0, true)
test.expectBool(f64.MinPositive > 0.0, true)
test.expectBool((f32.MinPositive).isFinite(), true)
test.expectBool((f64.MinPositive).isFinite(), true)
test.expectBool(f64.MinPositive < 1.0, true)

// Epsilon is the gap between 1.0 and the next representable value.
test.expectBool(f32.Epsilon > 0.0, true)
test.expectBool(f64.Epsilon > 0.0, true)
test.expectBool(1.0 + f64.Epsilon > 1.0, true)
