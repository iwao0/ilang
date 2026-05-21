/// C standard `<math.h>` — IEEE-754 double-precision math.
///
/// `float` variants (with the `f` suffix) and `long double`
/// variants (with the `l` suffix) follow the same shape and can be
/// added on demand. ilang's own `f32` / `f64` types map cleanly to
/// the `double` overloads here without manual casts.

@extern(C, "c") {
    // ── Trig ────────────────────────────────────────────────────

    @lib pub fn sin(x: f64): f64
    @lib pub fn cos(x: f64): f64
    @lib pub fn tan(x: f64): f64
    @lib pub fn asin(x: f64): f64
    @lib pub fn acos(x: f64): f64
    @lib pub fn atan(x: f64): f64
    @lib pub fn atan2(y: f64, x: f64): f64

    // ── Hyperbolic ──────────────────────────────────────────────

    @lib pub fn sinh(x: f64): f64
    @lib pub fn cosh(x: f64): f64
    @lib pub fn tanh(x: f64): f64

    // ── Exponential / logarithmic ───────────────────────────────

    @lib pub fn exp(x: f64): f64
    @lib pub fn exp2(x: f64): f64
    @lib pub fn log(x: f64): f64
    @lib pub fn log2(x: f64): f64
    @lib pub fn log10(x: f64): f64

    // ── Power / root ────────────────────────────────────────────

    @lib pub fn pow(x: f64, y: f64): f64
    @lib pub fn sqrt(x: f64): f64
    @lib pub fn cbrt(x: f64): f64
    @lib pub fn hypot(x: f64, y: f64): f64

    // ── Rounding / sign ─────────────────────────────────────────

    @lib pub fn floor(x: f64): f64
    @lib pub fn ceil(x: f64): f64
    @lib pub fn round(x: f64): f64
    @lib pub fn trunc(x: f64): f64
    @lib pub fn fabs(x: f64): f64
    @lib pub fn copysign(x: f64, y: f64): f64
    @lib pub fn fmod(x: f64, y: f64): f64
    @lib pub fn remainder(x: f64, y: f64): f64

    // ── Classification ──────────────────────────────────────────

    /// 1 if `x` is +∞ or -∞, 0 otherwise.
    @lib pub fn isinf(x: f64): i32
    /// 1 if `x` is NaN, 0 otherwise.
    @lib pub fn isnan(x: f64): i32

    // ── float (f32) variants — most-used subset ─────────────────

    @lib pub fn sinf(x: f32): f32
    @lib pub fn cosf(x: f32): f32
    @lib pub fn tanf(x: f32): f32
    @lib pub fn expf(x: f32): f32
    @lib pub fn logf(x: f32): f32
    @lib pub fn powf(x: f32, y: f32): f32
    @lib pub fn sqrtf(x: f32): f32
    @lib pub fn fabsf(x: f32): f32
}
