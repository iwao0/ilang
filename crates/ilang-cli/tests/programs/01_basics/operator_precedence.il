// Operator-precedence boundary cases — shift / bitwise / arithmetic
// combinations where the wrong grouping would give a different
// answer than the spec says.
use std.test as test

// Shift binds tighter than `|` and `&`.
test.expect(1 | 2 << 1, 5)            // 1 | (2<<1)
test.expect(0xF0 | 0x0F << 4, 0xF0)   // 0xF0 | (0x0F<<4) == 0xF0 | 0xF0
test.expect(0xFF & 0x0F << 4, 0xF0)   // 0xFF & (0x0F<<4)

// Shift binds looser than `+` and `-`.
test.expect(1 + 1 << 2, 8)            // (1+1)<<2
test.expect(8 >> 1 + 1, 2)            // 8 >> (1+1)
test.expect(3 - 1 << 2, 8)            // (3-1)<<2

// `&` binds tighter than `^`, `^` tighter than `|`.
test.expect(0x0F | 0xF0 & 0xFF, 0xFF)        // 0x0F | (0xF0&0xFF)
test.expect(0x0F ^ 0x33 | 0xC0, 0xFC)        // (0x0F^0x33) | 0xC0 == 0x3C|0xC0
test.expect(0xF0 & 0x33 ^ 0x0F, 0x3F)        // (0xF0&0x33) ^ 0x0F == 0x30^0x0F

// `*` / `/` / `%` bind tighter than `+` / `-`.
test.expect(1 + 2 * 3, 7)
test.expect(20 - 6 / 3, 18)
test.expect(7 + 9 % 4, 8)

// Unary `-` binds tighter than every binary op.
test.expect(-2 * 3, -6)
test.expect(-2 + 3, 1)
test.expect(-(1 + 2) * 3, -9)

// Comparison binds looser than arithmetic.
test.expectBool(1 + 1 == 2, true)
test.expectBool(3 * 2 < 10, true)

// `&&` binds tighter than `||`.
test.expectBool(true || false && false, true)
test.expectBool((true || false) && false, false)
