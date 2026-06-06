// Per-width Min / Max associated constants on every signed and
// unsigned integer type, per syntax.md §3 "Integer associated
// constants".
use std.test as test

// Signed: Min is the most negative two's-complement value, Max is
// the largest positive.
test.expect(i8.Min as i64, -128)
test.expect(i8.Max as i64, 127)
test.expect(i16.Min as i64, -32768)
test.expect(i16.Max as i64, 32767)
test.expect(i32.Min as i64, -2147483648)
test.expect(i32.Max as i64, 2147483647)
test.expect(i64.Min, -9223372036854775808)
test.expect(i64.Max, 9223372036854775807)

// Unsigned: Min is always 0, Max is all-bits-set in the width.
test.expect(u8.Min as i64, 0)
test.expect(u8.Max as i64, 255)
test.expect(u16.Min as i64, 0)
test.expect(u16.Max as i64, 65535)
test.expect(u32.Min as i64, 0)
test.expect(u32.Max as i64, 4294967295)
test.expect(u64.Min as i64, 0)
// u64.Max reinterpreted as i64 is -1 (all bits set).
test.expect(u64.Max as i64, -1)
