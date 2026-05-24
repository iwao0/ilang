// Integer boundaries: i64 MIN/MAX literals parse and round-trip.
// Two's-complement wrap on `MAX + 1`. Cast-then-wrap at narrower
// widths — including the i8 and u8 boundaries that aren't otherwise
// exercised by `const_lit_fits.il` (which only checks accepted forms).
use std.test

let max64: i64 = 9223372036854775807
let min64: i64 = -9223372036854775808

test.expect(max64 - 1, 9223372036854775806)
test.expect(min64 + 1, -9223372036854775807)
test.expectBool(max64 > min64, true)
test.expectBool(min64 < 0, true)

// MAX + 1 wraps to MIN.
test.expect(max64 + 1, min64)
// MIN - 1 wraps to MAX.
test.expect(min64 - 1, max64)

// u8 wraparound through cast.
let u: u8 = 255
let inc: u8 = (u as i64 + 1) as u8
test.expect(inc as i64, 0)

let dec: u8 = (0_u8 as i64 - 1) as u8
test.expect(dec as i64, 255)

// i8 boundaries.
test.expect((127 + 1) as i8 as i64, -128)
test.expect((-128 - 1) as i8 as i64, 127)

// Modulo where both operands are negative — sign follows the dividend.
test.expect(-17 % -5, -2)
test.expect(17 % -5, 2)
