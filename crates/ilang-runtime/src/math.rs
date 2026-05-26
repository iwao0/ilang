//! `math.*` stdlib bindings.
//!
//! Symbol names use `export_name = "$math.X"` so they match the
//! JIT-emitted symbol the codegen looks up via the `use math` form.
//! AOT links against these the same way it links any other runtime
//! helper, so calling `math.sin(...)` in ilang works in both
//! backends without per-call dispatch logic.

macro_rules! math_unary {
    ($name:ident, $sym:expr, $body:expr) => {
        #[unsafe(export_name = $sym)]
        pub extern "C" fn $name(x: f64) -> f64 { $body(x) }
    };
}
// Symbol names carry the `$` sigil so they match what the AOT
// codegen emits for `@intrinsic("math.X")` (the sigil keeps the
// runtime helper's symbol out of the ilang identifier namespace).
// Missing the `$` here used to leave AOT builds with unresolved
// `_$math.sin` / `_$math.cos` etc. at link time even though the
// JIT resolved them via dlsym (which doesn't see the export_name).
math_unary!(math_sin,   "$math.sin",   f64::sin);
math_unary!(math_cos,   "$math.cos",   f64::cos);
math_unary!(math_tan,   "$math.tan",   f64::tan);
math_unary!(math_asin,  "$math.asin",  f64::asin);
math_unary!(math_acos,  "$math.acos",  f64::acos);
math_unary!(math_atan,  "$math.atan",  f64::atan);
math_unary!(math_sqrt,  "$math.sqrt",  f64::sqrt);
math_unary!(math_exp,   "$math.exp",   f64::exp);
math_unary!(math_ln,    "$math.ln",    f64::ln);
math_unary!(math_log10, "$math.log10", f64::log10);
math_unary!(math_log2,  "$math.log2",  f64::log2);
math_unary!(math_floor, "$math.floor", f64::floor);
math_unary!(math_ceil,  "$math.ceil",  f64::ceil);
math_unary!(math_round, "$math.round", f64::round);
math_unary!(math_abs,   "$math.abs",   f64::abs);
math_unary!(math_sign,  "$math.sign",  f64::signum);
math_unary!(math_trunc, "$math.trunc", f64::trunc);
math_unary!(math_cbrt,  "$math.cbrt",  f64::cbrt);
math_unary!(math_sinh,  "$math.sinh",  f64::sinh);
math_unary!(math_cosh,  "$math.cosh",  f64::cosh);
math_unary!(math_tanh,  "$math.tanh",  f64::tanh);
math_unary!(math_asinh, "$math.asinh", f64::asinh);
math_unary!(math_acosh, "$math.acosh", f64::acosh);
math_unary!(math_atanh, "$math.atanh", f64::atanh);

#[unsafe(export_name = "$math.atan2")]
pub extern "C" fn math_atan2(y: f64, x: f64) -> f64 { y.atan2(x) }

#[unsafe(export_name = "$math.pow")]
pub extern "C" fn math_pow(x: f64, y: f64) -> f64 { x.powf(y) }

/// 2-argument hypotenuse `sqrt(a*a + b*b)`, computed via the host's
/// IEEE-754-safe path so overflow / underflow midway through doesn't
/// poison the result. JS's `Math.hypot` is variadic; ilang sticks to
/// two arguments and lets callers fold over arrays themselves.
#[unsafe(export_name = "$math.hypot")]
pub extern "C" fn math_hypot(a: f64, b: f64) -> f64 { a.hypot(b) }

// --------------------------------------------------------------------
// IEEE-754 predicates (`.isFinite()` / `.isNaN()` on f32 / f64)
// --------------------------------------------------------------------
//
// Cranelift's calling convention treats f32 / f64 args as float-
// register passes, so the host needs a per-width entry point — a
// single i64-shaped helper would mis-receive the float. Returning
// `i64` (0 / 1) keeps the result on the integer-register ABI so
// downstream MIR can `ireduce` to `i8` for `Bool`.

#[unsafe(export_name = "$math.isFinite_f32")]
pub extern "C" fn math_is_finite_f32(x: f32) -> i64 {
    if x.is_finite() { 1 } else { 0 }
}

#[unsafe(export_name = "$math.isFinite_f64")]
pub extern "C" fn math_is_finite_f64(x: f64) -> i64 {
    if x.is_finite() { 1 } else { 0 }
}

#[unsafe(export_name = "$math.isNaN_f32")]
pub extern "C" fn math_is_nan_f32(x: f32) -> i64 {
    if x.is_nan() { 1 } else { 0 }
}

#[unsafe(export_name = "$math.isNaN_f64")]
pub extern "C" fn math_is_nan_f64(x: f64) -> i64 {
    if x.is_nan() { 1 } else { 0 }
}

/// `math.random()` — uniform `f64` in `[0.0, 1.0)`, matching JS's
/// `Math.random()`. Delegates to `rand`'s thread-local generator
/// (`ThreadRng`), which auto-seeds from the OS RNG on first use
/// and keeps its state per thread. Same `Math.random()` quality
/// bar: good enough for scripting / games / Monte Carlo, not for
/// cryptography (`ThreadRng` itself is cryptographically secure,
/// but callers who need that contract should use the OS APIs
/// directly rather than relying on this one staying that way).
#[unsafe(export_name = "$math.random")]
pub extern "C" fn math_random() -> f64 {
    use rand::Rng;
    rand::rng().random_range(0.0..1.0)
}
