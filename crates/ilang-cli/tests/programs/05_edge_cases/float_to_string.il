// `.toString()` on f32 / f64 — pre-fix the MIR lowerer routed the
// call to a `float_to_string` builtin that wasn't declared in
// codegen, so any `(1.0).toString()` aborted with
// "unregistered builtin `float_to_string`". The fix is per-width
// entry points (`$string.fromF32` / `$string.fromF64`) so
// cranelift's float-arg ABI gets the value via the f32 / f64
// register file, matching the `$math.isFinite_f32` / `_f64`
// pattern.
//
// Output format must match `console.log` / template literals
// (`$fmt.f64`): integral floats keep the trailing `.0`, NaN /
// ±Infinity get the JS-style names.
use std.test as test

// f64 path
test.expectStr((1.0).toString(), "1.0")
test.expectStr((3.14).toString(), "3.14")
test.expectStr((-2.5).toString(), "-2.5")
test.expectStr((0.0).toString(), "0.0")

// Beyond 1e16 the formatter drops to Rust's default `{f64}` and the
// `.0` suffix is gone — matches JS's `(1e20).toString()` which also
// prints `"100000000000000000000"`.
let big: f64 = 1.0e20
test.expectStr(big.toString(), "100000000000000000000")

// IEEE-754 corner cases
let nan: f64 = 0.0 / 0.0
test.expectStr(nan.toString(), "NaN")
let inf: f64 = 1.0 / 0.0
test.expectStr(inf.toString(), "Infinity")
test.expectStr((-inf).toString(), "-Infinity")

// f32 path — same formatting after promoting to f64.
let f: f32 = 2.5
test.expectStr(f.toString(), "2.5")
let g: f32 = -0.5
test.expectStr(g.toString(), "-0.5")
