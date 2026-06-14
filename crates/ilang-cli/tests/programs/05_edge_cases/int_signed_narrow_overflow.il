// Pins the documented integer-overflow model (docs/syntax.md
// "Integer overflow"). Arithmetic never panics and wraps
// two's-complement, but the WIDTH it wraps at depends on the operation's
// operand type and where the value lands:
//   - i64 wraps at 64 bits;
//   - unsigned narrow (u8/u16/u32) wraps at its declared width on every op;
//   - signed narrow (i8/i16/i32) arithmetic on narrow-TYPED operands is
//     held at 64-bit width and is NOT re-wrapped — a value only wraps to
//     a narrow signed width when it crosses a narrowing cast from a wider
//     type, an implicit i64-narrowing assignment, or a narrow-cell store.
// This asymmetry (signed narrow local arithmetic not wrapping, unsigned
// does) is an accepted design choice, documented rather than "fixed".
use std.test as test

// i64 wraps at 64 bits.
let max64: i64 = 9223372036854775807
test.expect(max64 + 1, -9223372036854775807 - 1)

// Unsigned narrow wraps at its declared width on every operation.
test.expect(((4000000000 as u32) + (500000000 as u32)) as i64, 205032704)
test.expect(((250 as u8) + (10 as u8)) as i64, 4)
test.expect(((65000 as u16) + (1000 as u16)) as i64, 464)

// Signed narrow arithmetic on narrow-TYPED operands stays 64-bit:
// the result is not re-wrapped to 32 bits, so it can sit outside the
// i32 range and read back as a positive i64.
let a32: i32 = 2000000000 as i32
let b32: i32 = 500000000 as i32
let c32: i32 = a32 + b32
test.expect(c32 as i64, 2500000000)
test.expectBool(c32 < (0 as i32), false)

// Bare integer literals are i64, so this sum wraps at 64 bits and then
// the narrowing assignment to i32 wraps to the i32 range.
let x32: i32 = 2000000000 + 500000000
test.expect(x32 as i64, -1794967296)

// A narrowing cast FROM a wider type wraps.
let big: i64 = 2500000000
test.expect((big as i32) as i64, -1794967296)

// A store into a narrow cell wraps.
let arr: i32[] = [2000000000 as i32]
arr[0] = arr[0] + (500000000 as i32)
test.expect(arr[0] as i64, -1794967296)

// Round-tripping a narrow-typed sum through i64 forces the wrap, since
// `(a32 + b32) as i32` alone is a no-op (the sum is already i32).
test.expect(((a32 as i64 + b32) as i32) as i64, -1794967296)
