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
    @lib pub fn asinf(x: f32): f32
    @lib pub fn acosf(x: f32): f32
    @lib pub fn atanf(x: f32): f32
    @lib pub fn atan2f(y: f32, x: f32): f32
    @lib pub fn sinhf(x: f32): f32
    @lib pub fn coshf(x: f32): f32
    @lib pub fn tanhf(x: f32): f32
    @lib pub fn expf(x: f32): f32
    @lib pub fn exp2f(x: f32): f32
    @lib pub fn logf(x: f32): f32
    @lib pub fn log2f(x: f32): f32
    @lib pub fn log10f(x: f32): f32
    @lib pub fn powf(x: f32, y: f32): f32
    @lib pub fn sqrtf(x: f32): f32
    @lib pub fn cbrtf(x: f32): f32
    @lib pub fn hypotf(x: f32, y: f32): f32
    @lib pub fn floorf(x: f32): f32
    @lib pub fn ceilf(x: f32): f32
    @lib pub fn roundf(x: f32): f32
    @lib pub fn truncf(x: f32): f32
    @lib pub fn fabsf(x: f32): f32
    @lib pub fn copysignf(x: f32, y: f32): f32
    @lib pub fn fmodf(x: f32, y: f32): f32

    // ── C99 specials ────────────────────────────────────────────

    /// `log(1 + x)` computed accurately for `|x|` small.
    @lib pub fn log1p(x: f64): f64
    /// `exp(x) - 1` computed accurately for `|x|` small.
    @lib pub fn expm1(x: f64): f64

    /// Gamma function (Γ(x)).
    @lib pub fn tgamma(x: f64): f64
    /// `log(|tgamma(x)|)`.
    @lib pub fn lgamma(x: f64): f64

    /// Error function and its complement.
    @lib pub fn erf(x: f64): f64
    @lib pub fn erfc(x: f64): f64

    // ── Min / max / dim ─────────────────────────────────────────

    @lib pub fn fmin(x: f64, y: f64): f64
    @lib pub fn fmax(x: f64, y: f64): f64
    /// `max(x - y, 0)`. NaN-propagating.
    @lib pub fn fdim(x: f64, y: f64): f64
    /// Fused multiply-add: `x*y + z` with one rounding step.
    @lib pub fn fma(x: f64, y: f64, z: f64): f64

    // ── Rounding (extended) ─────────────────────────────────────

    /// Round to integer in the current rounding mode, without
    /// raising the inexact exception.
    @lib pub fn nearbyint(x: f64): f64
    /// Same but may raise inexact.
    @lib pub fn rint(x: f64): f64
    @lib pub fn lround(x: f64): i64
    @lib pub fn llround(x: f64): i64
    @lib pub fn lrint(x: f64): i64
    @lib pub fn llrint(x: f64): i64

    // ── Exponent manipulation ───────────────────────────────────

    /// `x = m * 2^e` with `m ∈ [0.5, 1)`. Writes `e` to `*exp`.
    @lib pub fn frexp(x: f64, exp: *i32): f64
    /// `x * 2^exp`.
    @lib pub fn ldexp(x: f64, exp: i32): f64
    /// Integer + fractional split. Stores the integer part in `*iptr`.
    @lib pub fn modf(x: f64, iptr: *f64): f64
    /// Next representable double after `x` toward `y`.
    @lib pub fn nextafter(x: f64, y: f64): f64
    /// `x * FLT_RADIX^exp` computed efficiently.
    @lib pub fn scalbn(x: f64, exp: i32): f64

    // ── Classification (extended) ───────────────────────────────

    /// 1 if `x` is finite (not ±∞, not NaN), 0 otherwise.
    @lib pub fn isfinite(x: f64): i32
    /// 1 if `x` is normal (not zero, sub-normal, ∞, or NaN).
    @lib pub fn isnormal(x: f64): i32
    /// 1 if the sign bit is set (negative or -0).
    @lib pub fn signbit(x: f64): i32
}
