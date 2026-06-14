// Right shift picks its opcode by the operand's signedness: a SIGNED
// shift is arithmetic (sign-extends the high bits), an UNSIGNED shift is
// logical (zero-fills). `bitwise.il` exercises `<<` / `>>` generally but
// not this distinction, which a codegen refactor could quietly break.
// Left shift is identical for both.
use std.test as test

// Signed (i64 / i32) `>>` is ARITHMETIC.
test.expect(-8 >> 1, -4)
test.expect(-1 >> 5, -1)        // all-ones stays all-ones
test.expect(-256 >> 4, -16)
let i32n: i32 = -16 as i32
test.expect((i32n >> (1 as i32)) as i64, -8)

// Unsigned (u64 / u32) `>>` is LOGICAL — the top bit zero-fills.
let u: u64 = 0xFFFFFFFFFFFFFFF8 as u64
test.expect((u >> (1 as u64)) as i64, 0x7FFFFFFFFFFFFFFC)
let u32v: u32 = 0xFFFFFFF0 as u32
test.expect((u32v >> (1 as u32)) as i64, 0x7FFFFFF8)

// Left shift is signedness-independent.
test.expect(-1 << 4, -16)
test.expect(1 << 10, 1024)
