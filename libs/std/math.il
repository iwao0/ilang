/// Built-in `math` module — trigonometry, logs, roots, and other
/// f64 helpers. All extern fns are implemented in the host runtime;
/// calling code goes through the standard `use std.math` form.

/// Returns the sine of `x` (radians).
///
/// * `x` - angle in radians.
@intrinsic("math.sin")
pub fn sin(x: f64): f64

/// Returns the cosine of `x` (radians).
///
/// * `x` - angle in radians.
@intrinsic("math.cos")
pub fn cos(x: f64): f64

/// Returns the tangent of `x` (radians).
///
/// * `x` - angle in radians.
@intrinsic("math.tan")
pub fn tan(x: f64): f64

/// Inverse sine. Result is in `[-π/2, π/2]`.
///
/// * `x` - value in `[-1, 1]`. NaN otherwise.
@intrinsic("math.asin")
pub fn asin(x: f64): f64

/// Inverse cosine. Result is in `[0, π]`.
///
/// * `x` - value in `[-1, 1]`. NaN otherwise.
@intrinsic("math.acos")
pub fn acos(x: f64): f64

/// Inverse tangent. Result is in `[-π/2, π/2]`.
///
/// * `x` - any real number.
@intrinsic("math.atan")
pub fn atan(x: f64): f64

/// Two-argument arctangent of `y/x`, taking the signs of both into account.
///
/// * `y` - numerator (vertical component).
/// * `x` - denominator (horizontal component).
@intrinsic("math.atan2")
pub fn atan2(y: f64, x: f64): f64

/// Square root of `x`. Returns NaN for negative inputs.
///
/// * `x` - non-negative value.
@intrinsic("math.sqrt")
pub fn sqrt(x: f64): f64

/// Raises `base` to the `exp` power.
///
/// * `base` - the base.
/// * `exp` - the exponent.
@intrinsic("math.pow")
pub fn pow(base: f64, exp: f64): f64

/// `e` raised to the `x` power.
///
/// * `x` - the exponent.
@intrinsic("math.exp")
pub fn exp(x: f64): f64

/// Natural logarithm (base `e`).
///
/// * `x` - positive value. NaN for `x < 0`, -∞ for `x == 0`.
@intrinsic("math.ln")
pub fn ln(x: f64): f64

/// Base-10 logarithm.
///
/// * `x` - positive value. NaN for `x < 0`, -∞ for `x == 0`.
@intrinsic("math.log10")
pub fn log10(x: f64): f64

/// Base-2 logarithm.
///
/// * `x` - positive value. NaN for `x < 0`, -∞ for `x == 0`.
@intrinsic("math.log2")
pub fn log2(x: f64): f64

/// Largest integer ≤ `x`.
///
/// * `x` - any real number.
@intrinsic("math.floor")
pub fn floor(x: f64): f64

/// Smallest integer ≥ `x`.
///
/// * `x` - any real number.
@intrinsic("math.ceil")
pub fn ceil(x: f64): f64

/// Round `x` to the nearest integer (half away from zero).
///
/// * `x` - any real number.
@intrinsic("math.round")
pub fn round(x: f64): f64

/// Absolute value of `x`.
///
/// * `x` - any real number.
@intrinsic("math.abs")
pub fn abs(x: f64): f64

/// Pseudo-random `f64` in `[0.0, 1.0)`. Mirrors JavaScript's
/// `Math.random()`: same uniform half-open interval, same "good
/// enough for scripting, not for cryptography" quality bar. Each
/// thread keeps its own stream, lazily seeded from the wall clock
/// on first call.
@intrinsic("math.random")
pub fn random(): f64

/// Sign of `x`: `1.0` when positive, `-1.0` when negative, `0.0` when
/// `±0.0`, and `NaN` when `x` is NaN. Matches JS's `Math.sign` (NOT
/// Rust's `f64::signum`, which returns `±1.0` for `±0.0`).
@intrinsic("math.sign")
pub fn sign(x: f64): f64

/// Truncates toward zero — drops the fractional part without
/// rounding. `trunc(3.7) == 3.0`, `trunc(-3.7) == -3.0`.
@intrinsic("math.trunc")
pub fn trunc(x: f64): f64

/// Cube root of `x`, defined for every real `x` (negative inputs
/// return a negative root, unlike `pow(x, 1.0/3.0)`).
@intrinsic("math.cbrt")
pub fn cbrt(x: f64): f64

/// 2D hypotenuse `sqrt(a*a + b*b)` computed via the host's
/// IEEE-754-safe path so intermediate overflow / underflow doesn't
/// poison the result. JS's `Math.hypot` is variadic; fold yourself
/// when more than two values are involved.
@intrinsic("math.hypot")
pub fn hypot(a: f64, b: f64): f64

/// Hyperbolic sine.
@intrinsic("math.sinh")
pub fn sinh(x: f64): f64

/// Hyperbolic cosine.
@intrinsic("math.cosh")
pub fn cosh(x: f64): f64

/// Hyperbolic tangent.
@intrinsic("math.tanh")
pub fn tanh(x: f64): f64

/// Inverse hyperbolic sine.
@intrinsic("math.asinh")
pub fn asinh(x: f64): f64

/// Inverse hyperbolic cosine. Domain `[1, ∞)`; NaN otherwise.
@intrinsic("math.acosh")
pub fn acosh(x: f64): f64

/// Inverse hyperbolic tangent. Domain `(-1, 1)`; NaN otherwise.
@intrinsic("math.atanh")
pub fn atanh(x: f64): f64

// --------------------------------------------------------------------
// Pure-ilang helpers — composed from the primitives above. No host
// dispatch needed; the JIT inlines / folds these alongside user code.

/// Smaller of `a` and `b`. A NaN input propagates (matching JS
/// `Math.min`), from EITHER side — `min(nan, x) == min(x, nan) == nan`.
/// The bare `a < b` form was order-dependent: it propagated a NaN `b`
/// but silently returned `a` for a NaN `a`.
pub fn min(a: f64, b: f64): f64 {
    if a.isNaN() { a } elif b.isNaN() { b } elif a < b { a } else { b }
}

/// Larger of `a` and `b`. A NaN input propagates (matching JS
/// `Math.max`), from EITHER side — `max(nan, x) == max(x, nan) == nan`.
pub fn max(a: f64, b: f64): f64 {
    if a.isNaN() { a } elif b.isNaN() { b } elif a > b { a } else { b }
}

/// Clamp `v` to the closed interval `[lo, hi]`. Returns `lo` when
/// `v < lo`, `hi` when `v > hi`, `v` otherwise. Behaviour for
/// `lo > hi` is unspecified (callers are expected to keep them in
/// order).
pub fn clamp(v: f64, lo: f64, hi: f64): f64 {
    if v < lo { lo } elif v > hi { hi } else { v }
}

/// Linear interpolation from `a` to `b` at parameter `t`. `t == 0`
/// yields `a`, `t == 1` yields `b`. Doesn't clamp — values outside
/// `[0, 1]` extrapolate. Use `clamp(t, 0.0, 1.0)` first if you need
/// the clamped form.
pub fn lerp(a: f64, b: f64, t: f64): f64 {
    a + (b - a) * t
}

/// Hermite-smoothed interpolation across `[edge0, edge1]`. Returns
/// `0.0` when `x <= edge0`, `1.0` when `x >= edge1`, and the
/// `3t² − 2t³` curve in between — same shape as GLSL's
/// `smoothstep`.
pub fn smoothstep(edge0: f64, edge1: f64, x: f64): f64 {
    let t = clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0)
    t * t * (3.0 - 2.0 * t)
}

/// Linearly remap `v` from `[fromLo, fromHi]` to `[toLo, toHi]`.
/// Useful for normalizing input to a different range
/// (e.g. mapping a `[0, 100]` HP gauge to `[0.0, 1.0]` shader
/// uniform). Doesn't clamp — wrap in `clamp` if needed.
pub fn remap(v: f64, fromLo: f64, fromHi: f64, toLo: f64, toHi: f64): f64 {
    toLo + (v - fromLo) * (toHi - toLo) / (fromHi - fromLo)
}

// Compile-time folded constants — inlined at every reference.

/// Ratio of a circle's circumference to its diameter (π).
pub const pi: f64 = 3.141592653589793
/// Base of the natural logarithm (Euler's number).
pub const e: f64 = 2.718281828459045
/// Natural log of 2.
pub const ln2: f64 = 0.6931471805599453
/// Natural log of 10.
pub const ln10: f64 = 2.302585092994046
/// Base-2 log of e.
pub const log2e: f64 = 1.4426950408889634
/// Base-10 log of e.
pub const log10e: f64 = 0.4342944819032518
/// Square root of 2.
pub const sqrt2: f64 = 1.4142135623730951
/// Square root of 1/2.
pub const sqrt1_2: f64 = 0.7071067811865476
