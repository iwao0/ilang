// Built-in `math` module — trigonometry, logs, roots, and other
// f64 helpers. All extern fns are implemented in the host runtime;
// calling code goes through the standard `use math` form.

@extern(C) {
    /// Returns the sine of `x` (radians).
    ///
    /// * `x` - angle in radians.
    pub fn sin(x: f64): f64
    /// Returns the cosine of `x` (radians).
    ///
    /// * `x` - angle in radians.
    pub fn cos(x: f64): f64
    /// Returns the tangent of `x` (radians).
    ///
    /// * `x` - angle in radians.
    pub fn tan(x: f64): f64
    /// Inverse sine. Result is in `[-π/2, π/2]`.
    ///
    /// * `x` - value in `[-1, 1]`. NaN otherwise.
    pub fn asin(x: f64): f64
    /// Inverse cosine. Result is in `[0, π]`.
    ///
    /// * `x` - value in `[-1, 1]`. NaN otherwise.
    pub fn acos(x: f64): f64
    /// Inverse tangent. Result is in `[-π/2, π/2]`.
    ///
    /// * `x` - any real number.
    pub fn atan(x: f64): f64
    /// Two-argument arctangent of `y/x`, taking the signs of both into account.
    ///
    /// * `y` - numerator (vertical component).
    /// * `x` - denominator (horizontal component).
    pub fn atan2(y: f64, x: f64): f64
    /// Square root of `x`. Returns NaN for negative inputs.
    ///
    /// * `x` - non-negative value.
    pub fn sqrt(x: f64): f64
    /// Raises `base` to the `exp` power.
    ///
    /// * `base` - the base.
    /// * `exp` - the exponent.
    pub fn pow(base: f64, exp: f64): f64
    /// `e` raised to the `x` power.
    ///
    /// * `x` - the exponent.
    pub fn exp(x: f64): f64
    /// Natural logarithm (base `e`).
    ///
    /// * `x` - positive value. NaN for `x < 0`, -∞ for `x == 0`.
    pub fn ln(x: f64): f64
    /// Base-10 logarithm.
    ///
    /// * `x` - positive value. NaN for `x < 0`, -∞ for `x == 0`.
    pub fn log10(x: f64): f64
    /// Base-2 logarithm.
    ///
    /// * `x` - positive value. NaN for `x < 0`, -∞ for `x == 0`.
    pub fn log2(x: f64): f64
    /// Largest integer ≤ `x`.
    ///
    /// * `x` - any real number.
    pub fn floor(x: f64): f64
    /// Smallest integer ≥ `x`.
    ///
    /// * `x` - any real number.
    pub fn ceil(x: f64): f64
    /// Round `x` to the nearest integer (half away from zero).
    ///
    /// * `x` - any real number.
    pub fn round(x: f64): f64
    /// Absolute value of `x`.
    ///
    /// * `x` - any real number.
    pub fn abs(x: f64): f64
}

// Constants — inlined at compile time by the loader's const pass.

/// Ratio of a circle's circumference to its diameter (π).
pub const pi: f64 = 3.141592653589793
/// Base of the natural logarithm (Euler's number).
pub const e: f64 = 2.718281828459045
