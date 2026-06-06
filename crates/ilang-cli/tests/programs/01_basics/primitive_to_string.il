// `.toString()` on primitives — integers, floats, bool. Float
// formatting matches `console.log` (integral values print as `N.0`).
use std.test as test

// Integers — same digits as `console.log`.
test.expectStr((0).toString(), "0")
test.expectStr((42).toString(), "42")
test.expectStr((-7).toString(), "-7")
test.expectStr((1_000_000).toString(), "1000000")

// Per-width signed / unsigned integers.
test.expectStr((127_i8).toString(), "127")
test.expectStr((-128_i8).toString(), "-128")
test.expectStr((255_u8).toString(), "255")
test.expectStr((65535_u16).toString(), "65535")
test.expectStr((-2147483648_i32).toString(), "-2147483648")

// Floats — integral values keep a trailing `.0`.
test.expectStr((0.0).toString(), "0.0")
test.expectStr((1.0).toString(), "1.0")
test.expectStr((-2.0).toString(), "-2.0")
test.expectStr((3.14).toString(), "3.14")
test.expectStr((-0.5).toString(), "-0.5")

// f32 routes through the same formatter (promoted to f64).
test.expectStr((1.5_f32).toString(), "1.5")

// Bool.
test.expectStr(true.toString(), "true")
test.expectStr(false.toString(), "false")
