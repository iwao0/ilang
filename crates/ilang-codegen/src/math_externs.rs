//! Host-side math FFI helpers exposed to JITed code as `@extern fn`s.
//! Names match the qualified form produced by the loader (`math.sin`,
//! etc.) so calls land on the right symbol via Cranelift's import
//! linkage.

use cranelift_jit::JITBuilder;

extern "C" fn math_sin(x: f64) -> f64 { x.sin() }
extern "C" fn math_cos(x: f64) -> f64 { x.cos() }
extern "C" fn math_tan(x: f64) -> f64 { x.tan() }
extern "C" fn math_asin(x: f64) -> f64 { x.asin() }
extern "C" fn math_acos(x: f64) -> f64 { x.acos() }
extern "C" fn math_atan(x: f64) -> f64 { x.atan() }
extern "C" fn math_atan2(y: f64, x: f64) -> f64 { y.atan2(x) }
extern "C" fn math_sqrt(x: f64) -> f64 { x.sqrt() }
extern "C" fn math_pow(b: f64, e: f64) -> f64 { b.powf(e) }
extern "C" fn math_exp(x: f64) -> f64 { x.exp() }
extern "C" fn math_ln(x: f64) -> f64 { x.ln() }
extern "C" fn math_log10(x: f64) -> f64 { x.log10() }
extern "C" fn math_log2(x: f64) -> f64 { x.log2() }
extern "C" fn math_floor(x: f64) -> f64 { x.floor() }
extern "C" fn math_ceil(x: f64) -> f64 { x.ceil() }
extern "C" fn math_round(x: f64) -> f64 { x.round() }
extern "C" fn math_abs(x: f64) -> f64 { x.abs() }

pub(crate) fn register_math_symbols(builder: &mut JITBuilder) {
    builder.symbol("math.sin", math_sin as *const u8);
    builder.symbol("math.cos", math_cos as *const u8);
    builder.symbol("math.tan", math_tan as *const u8);
    builder.symbol("math.asin", math_asin as *const u8);
    builder.symbol("math.acos", math_acos as *const u8);
    builder.symbol("math.atan", math_atan as *const u8);
    builder.symbol("math.atan2", math_atan2 as *const u8);
    builder.symbol("math.sqrt", math_sqrt as *const u8);
    builder.symbol("math.pow", math_pow as *const u8);
    builder.symbol("math.exp", math_exp as *const u8);
    builder.symbol("math.ln", math_ln as *const u8);
    builder.symbol("math.log10", math_log10 as *const u8);
    builder.symbol("math.log2", math_log2 as *const u8);
    builder.symbol("math.floor", math_floor as *const u8);
    builder.symbol("math.ceil", math_ceil as *const u8);
    builder.symbol("math.round", math_round as *const u8);
    builder.symbol("math.abs", math_abs as *const u8);
}
