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

#[unsafe(export_name = "$math.atan2")]
pub extern "C" fn math_atan2(y: f64, x: f64) -> f64 { y.atan2(x) }

#[unsafe(export_name = "$math.pow")]
pub extern "C" fn math_pow(x: f64, y: f64) -> f64 { x.powf(y) }
