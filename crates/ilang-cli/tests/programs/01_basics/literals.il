// All literal forms documented in syntax.md §1.
use std.test as test
// Decimal / hex / octal / binary / digit-underscore int literals.
test.expect(42, 42)
test.expect(0xff, 255)
test.expect(0o777, 511)
test.expect(0o10, 8)
test.expect(0b1011, 11)
test.expect(1_000_000, 1000000)

// Suffixed integer widths — coerced via `as i64` for the assertion.
test.expect(255_u8 as i64, 255)
test.expect(0xffff_u16 as i64, 65535)
test.expect(1_i32 as i64, 1)

// Negative typed literals retain their declared width.
test.expect(-1_i32 as i64, -1)
test.expect(-128_i8 as i64, -128)
test.expect(-32768_i16 as i64, -32768)
test.expect(-9223372036854775808_i64, -9223372036854775808)

// Float literals — natural and scientific. f32 explicit via suffix.
test.expectF64(3.14, 3.14)
test.expectF64(1.5e2, 150.0)
test.expectF64(2.5_f32 as f64, 2.5)

// Bool.
test.expectBool(true, true)
test.expectBool(false, false)

// String with escape sequences.
test.expectStr("hi", "hi")
test.expectStr("a\nb", "a\nb")
test.expectStr("a\tb", "a\tb")
test.expectStr("a\\b", "a\\b")
test.expectStr("a\"b", "a\"b")
test.expect("a\0b".length, 3)
