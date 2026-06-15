// Clean-round pin for unsigned-integer edges around round 132:
// - u64 compared against boundary literals (the literal adopts u64, so
//   the comparison is unsigned even when the literal is near / above
//   2^63),
// - narrow unsigned types (u8/u16/u32) doing literal div / mod / shift
//   (they zero-extend to a positive i64, so the signed op is already
//   correct — regression guard),
// - u64 <-> i64 casts and u64 narrowing.
//
// expect: ok
use std.test as test

let big: u64 = 18000000000000000000   // > 2^63

// --- u64 comparison with literals, including near / past the boundary ---
test.expectTrue(big > 100)
test.expectTrue(big > 9000000000000000000)        // literal just below 2^63
test.expectTrue(big >= 18000000000000000000)
test.expectTrue(big < 18000000000000000001)       // literal just above big
test.expectTrue(!(big < 5))
test.expectTrue(big != 0)
test.expectTrue(big == 18000000000000000000)

// --- narrow unsigned literal arithmetic (already correct; guard it) ---
let u32big: u32 = 4000000000                        // > 2^31
test.expectTrue((u32big / 2) == 2000000000)
test.expectTrue((u32big % 7) == 3)                  // 4000000000 % 7
test.expectTrue((u32big >> 1) == 2000000000)        // logical
let u16big: u16 = 60000
test.expectTrue((u16big / 2) == 30000)
let u8big: u8 = 250
test.expectTrue((u8big / 2) == 125)

// --- casts ---
test.expectTrue((big as i64) < 0)                   // top bit set -> negative i64
let neg: i64 = -1
test.expectTrue((neg as u64) > 1000000000000000000) // -1 -> 2^64-1
test.expectStr((neg as u64).toString(), "18446744073709551615")  // 2^64-1
let half: u64 = 9000000000000000000
test.expectTrue((half as i64) == 9000000000000000000)  // fits, positive
// narrowing keeps the low bits
let low: u32 = big as u32
test.expectTrue((low as u64) == (big % 4294967296))    // low 32 bits

console.log("ok")
