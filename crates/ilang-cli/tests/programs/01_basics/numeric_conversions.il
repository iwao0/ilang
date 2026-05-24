// Implicit widening / narrowing within same sign + explicit `as` casts.
use std.test

// Implicit: same-sign int widening + narrowing.
let n8: i8 = 100
let n16: i16 = n8           // i8 → i16 (widen, implicit)
let n32: i32 = n16          // i16 → i32
let n64: i64 = n32          // i32 → i64
test.expect(n64, 100)

let m32: i32 = 200_i32
let m8: i8 = m32 as i8      // narrow needs `as`? Actually same-sign is implicit
// Implicit narrowing of same-sign ints is allowed per syntax.md §2.
let m8b: i8 = m32           // also OK
test.expect(m8 as i64, 200 - 256)   // 200 doesn't fit in i8 → wraps to -56
test.expect(m8b as i64, m8 as i64)

// Implicit: int → float.
let f: f64 = 7
test.expectF64(f, 7.0)

// Implicit: f32 ↔ f64.
let g32: f32 = 1.25_f32
let g64: f64 = g32
test.expectF64(g64, 1.25)
let h32: f32 = g64
test.expectF64(h32 as f64, 1.25)

// Explicit `as` for sign-flips and float→int.
let u: u32 = 42_u32
let i: i32 = u as i32
test.expect(i as i64, 42)

let pi: f64 = 3.7
let pi_int = pi as i64       // truncates toward zero
test.expect(pi_int, 3)
test.expect(-3.9 as i64, -3)
