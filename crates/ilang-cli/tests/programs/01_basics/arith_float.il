// Float arithmetic. `%` is intentionally not supported on floats.
use std.test as test
test.expectF64(1.5 + 2.5, 4.0)
test.expectF64(10.0 - 0.25, 9.75)
test.expectF64(2.0 * 3.5, 7.0)
test.expectF64(7.0 / 2.0, 3.5)

// Int → float promotion: dividing an int literal by a float result.
test.expectF64(7.0 / 2, 3.5)
test.expectF64(1 + 2.5, 3.5)

// Unary minus.
test.expectF64(-2.5, -2.5)
test.expectF64(-(1.0 + 1.0), -2.0)

// Scientific notation.
test.expectF64(1.5e2, 150.0)
test.expectF64(2.5e-1, 0.25)

// f32 path: coerce to f64 for the assertion (only F64 expect helper exists).
let a: f32 = 1.5
let b: f32 = 2.0
test.expectF64((a + b) as f64, 3.5)
