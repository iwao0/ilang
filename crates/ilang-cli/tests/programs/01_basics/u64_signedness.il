// u64 values with the top bit set (≥ 2^63) must be treated as UNSIGNED
// everywhere — division / modulo / right-shift by an integer literal,
// and every formatting path (toString, console.log, array print,
// template `${}`). Comparisons were already unsigned.
//
// Two pre-existing bugs:
//   1. `u64_val / 2` (`% 2`, `>> 2`, …) used the SIGNED op: a bare
//      integer literal is the same width as u64, and the lowering's
//      cross-sign tie-break preferred the signed side. So `18e18 / 2`
//      gave a negative result instead of `9e18`. The literal now adopts
//      the u64 type (matching the checker), so the unsigned op is used.
//   2. toString / console.log / array print / `${}` rendered the i64
//      bit pattern as SIGNED decimal, so `18e18` printed as a negative
//      number. u64 now routes through unsigned formatters.
//
// expect: 18000000000000000000
// expect: 10000000000000000000 5
// expect: [10000000000000000000, 5]
use std.test as test

let big: u64 = 18000000000000000000   // > 2^63
let x: u64 = 10000000000000000000

// --- arithmetic by an integer literal stays unsigned ---
test.expectTrue((big / 2) == 9000000000000000000)
test.expectTrue((big % 7) == 4)
let top: u64 = 0x8000000000000000      // exactly 2^63
test.expectTrue((top >> 1) == 4611686018427387904)   // logical shift -> 2^62

// explicit-typed divisor agrees with the literal form
let two: u64 = 2
test.expectTrue((big / two) == (big / 2))

// --- formatting paths render unsigned ---
test.expectStr(big.toString(), "18000000000000000000")
test.expectStr(`${big}`, "18000000000000000000")

// --- comparisons remain unsigned (regression guard) ---
test.expectTrue(big > 5)
test.expectTrue(!(big < 5))
test.expectTrue(x < big)

// --- signed i64 ops are unaffected ---
let neg: i64 = -10
test.expect(neg / 2, -5)
test.expect(neg % 3, -1)
test.expect(-8 >> 1, -4)               // arithmetic shift

// print paths (checked against the `// expect:` lines above)
console.log(big)
console.log(x, 5)
let arr: u64[] = [x, 5]
console.log(arr)
