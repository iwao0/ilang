//! Host trampolines for the built-in `math` module. Each one wraps
//! a `f64` libstd method and is registered with the JIT under its
//! `math.<name>` symbol; the AOT path links them through the same
//! names exported from `ilang-runtime`.

pub(super) extern "C" fn host_atan2(y: f64, x: f64) -> f64 { y.atan2(x) }
pub(super) extern "C" fn host_pow(x: f64, y: f64) -> f64 { x.powf(y) }
pub(super) extern "C" fn host_sin(x: f64) -> f64 { x.sin() }
pub(super) extern "C" fn host_cos(x: f64) -> f64 { x.cos() }
pub(super) extern "C" fn host_tan(x: f64) -> f64 { x.tan() }
pub(super) extern "C" fn host_asin(x: f64) -> f64 { x.asin() }
pub(super) extern "C" fn host_acos(x: f64) -> f64 { x.acos() }
pub(super) extern "C" fn host_atan(x: f64) -> f64 { x.atan() }
pub(super) extern "C" fn host_sqrt(x: f64) -> f64 { x.sqrt() }
pub(super) extern "C" fn host_exp(x: f64) -> f64 { x.exp() }
pub(super) extern "C" fn host_ln(x: f64) -> f64 { x.ln() }
pub(super) extern "C" fn host_log10(x: f64) -> f64 { x.log10() }
pub(super) extern "C" fn host_log2(x: f64) -> f64 { x.log2() }
pub(super) extern "C" fn host_floor(x: f64) -> f64 { x.floor() }
pub(super) extern "C" fn host_ceil(x: f64) -> f64 { x.ceil() }
pub(super) extern "C" fn host_round(x: f64) -> f64 { x.round() }
pub(super) extern "C" fn host_abs(x: f64) -> f64 { x.abs() }
