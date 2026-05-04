// Built-in `math` module — trigonometry, logs, roots, and other
// f64 helpers. All extern fns are implemented in the host runtime;
// calling code goes through the standard `use math` form.

@extern(C) {
    fn sin(x: f64): f64
    fn cos(x: f64): f64
    fn tan(x: f64): f64
    fn asin(x: f64): f64
    fn acos(x: f64): f64
    fn atan(x: f64): f64
    fn atan2(y: f64, x: f64): f64
    fn sqrt(x: f64): f64
    fn pow(base: f64, exp: f64): f64
    fn exp(x: f64): f64
    fn ln(x: f64): f64
    fn log10(x: f64): f64
    fn log2(x: f64): f64
    fn floor(x: f64): f64
    fn ceil(x: f64): f64
    fn round(x: f64): f64
    fn abs(x: f64): f64
}

// Constants — inlined at compile time by the loader's const pass.
const pi: f64 = 3.141592653589793
const e: f64 = 2.718281828459045
