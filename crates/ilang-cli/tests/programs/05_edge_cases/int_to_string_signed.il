// `.toString()` on a signed integer must format the value with
// its sign, matching Rust's `i*::to_string`. Pre-fix the lowerer
// widened i8 / i16 / i32 args to i64 via uextend (zero-fill)
// before passing them to the host `__int_to_string` helper, so
// negative values displayed as their unsigned bit pattern
// (e.g. `i8 -1` → `"255"`).
//
// `as i64` widening uses sextend correctly today (verified by
// `cast_float_to_int_saturating.il`); only the `.toString()`
// path mishandled the sign.
use test

let a: i8 = -1
test.expectStr(a.toString(), "-1")

let b: i8 = -128
test.expectStr(b.toString(), "-128")

let c: i16 = -1
test.expectStr(c.toString(), "-1")

let d: i16 = -32768
test.expectStr(d.toString(), "-32768")

let e: i32 = -1
test.expectStr(e.toString(), "-1")

let f: i32 = -2147483648
test.expectStr(f.toString(), "-2147483648")

// i64 was correct already — included for the contrast.
let g: i64 = -1
test.expectStr(g.toString(), "-1")

let h: i64 = -9223372036854775807 - 1   // i64::MIN
test.expectStr(h.toString(), "-9223372036854775808")

// Unsigned types kept their previous (correct) display.
let p: u8 = 255u8
test.expectStr(p.toString(), "255")
let q: u16 = 65535u16
test.expectStr(q.toString(), "65535")
let r: u32 = 4294967295u32
test.expectStr(r.toString(), "4294967295")
