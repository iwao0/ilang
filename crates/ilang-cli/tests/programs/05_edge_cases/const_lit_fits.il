// Const declarations with explicit numeric annotations have their
// folded literal value range-checked at load time. Pre-fix
// `const X: i8 = 200` quietly substituted to `200 as i8` and
// produced -56 at runtime (the parity test in the harness would
// then accept either backend's wrapped value).
use test

const SMALL: i8 = 100
const NEG_MIN: i8 = -128
const POS_MAX: i8 = 127
const U8_MAX: u8 = 255
const U16_MAX: u16 = 65535
const I32_MIN: i32 = -2147483648

test.expect(SMALL as i64, 100)
test.expect(NEG_MIN as i64, -128)
test.expect(POS_MAX as i64, 127)
test.expect(U8_MAX as i64, 255)
test.expect(U16_MAX as i64, 65535)
test.expect(I32_MIN as i64, -2147483648)
