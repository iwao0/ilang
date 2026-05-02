// Built-in `math` module — trigonometry, logs, roots, and other
// f64 helpers. All `@extern` fns are implemented in the host
// runtime; calling code goes through the standard `use math` form.

@extern fn sin(x: f64): f64
@extern fn cos(x: f64): f64
@extern fn tan(x: f64): f64
@extern fn asin(x: f64): f64
@extern fn acos(x: f64): f64
@extern fn atan(x: f64): f64
@extern fn atan2(y: f64, x: f64): f64
@extern fn sqrt(x: f64): f64
@extern fn pow(base: f64, exp: f64): f64
@extern fn exp(x: f64): f64
@extern fn ln(x: f64): f64
@extern fn log10(x: f64): f64
@extern fn log2(x: f64): f64
@extern fn floor(x: f64): f64
@extern fn ceil(x: f64): f64
@extern fn round(x: f64): f64
@extern fn abs(x: f64): f64

// Constants — written as zero-arg fns so they share the same surface
// as the rest. (No top-level `let` in items yet.)
fn pi(): f64 { 3.141592653589793 }
fn e(): f64 { 2.718281828459045 }
