// Bitwise operators: & | ^ ~ << >> on integers.
use std.test

test.expect(0xff & 0x0f, 0x0f)
test.expect(0xf0 | 0x0f, 0xff)
test.expect(0xff ^ 0x0f, 0xf0)
test.expect(~0_i32 as i64, -1)            // ~0 in i32 → -1 sign-extended
test.expect(1 << 4, 16)
test.expect(0x100 >> 4, 16)

// Precedence: shift > comparisons > & > ^ > | > && > || .
test.expect(1 << 1 + 1, 4)                // shift binds looser than +
test.expect((1 << 2) | 1, 5)
test.expect(0xff & 0xf0 | 0x05, 0xf5)
test.expect(0xff & (0xf0 | 0x05), 0xf5)

// Compound assignments.
let x = 0xff
x &= 0x0f
test.expect(x, 0x0f)
x |= 0x30
test.expect(x, 0x3f)
x ^= 0x0a
test.expect(x, 0x35)
x <<= 2
test.expect(x, 0xd4)
x >>= 1
test.expect(x, 0x6a)
