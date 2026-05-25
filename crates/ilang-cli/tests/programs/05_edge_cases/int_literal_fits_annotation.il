// Integer literals that fit the target's width are accepted; ones
// that don't need to be rejected at type-check time. Pre-fix the
// general `assignable` int-narrowing rule fired before
// `int_literal_fits`, so `let a: i8 = 999` was silently accepted
// and wrapped at runtime.
use std.test as test
let a: i8 = 100
let b: i8 = -50
let c: u8 = 200
let d: i32 = -2147483648
let e: u32 = 4294967295
test.expect(a as i64, 100)
test.expect(b as i64, -50)
test.expect(c as i64, 200)
test.expect(d as i64, -2147483648)
test.expect(e as i64, 4294967295)
