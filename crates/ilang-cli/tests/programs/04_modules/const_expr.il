// const can hold a compile-time-evaluable expression.
use test

const N: i64 = 1 + 2 * 3                    // 7
const TWO_N: i64 = N * 2                     // 14 (refers to another const)
const NEG: i64 = -N                          // -7
const FLAG: bool = !(1 == 2) && (3 < 5)      // true
const HALF: f64 = 1.0 / 2.0                  // 0.5
const HELLO: string = "Hi, " + "World"
const SHIFT: i64 = 1 << 4                    // 16
const MASK: i64 = 0xFF & 0x3C                // 60
const FROM_INT: f64 = 42 as f64              // 42.0

test.expect(N, 7)
test.expect(TWO_N, 14)
test.expect(NEG, -7)
test.expectBool(FLAG, true)
test.expectF64(HALF, 0.5)
test.expectStr(HELLO, "Hi, World")
test.expect(SHIFT, 16)
test.expect(MASK, 60)
test.expectF64(FROM_INT, 42.0)
