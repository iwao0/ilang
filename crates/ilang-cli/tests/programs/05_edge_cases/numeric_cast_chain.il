// Multi-step `as` casts. `numeric_conversions.il` covers single-hop
// casts with in-range values; this fixture stresses chains where
// each hop changes width and/or sign and may truncate.
use test

// Down-then-up: 1000 → i8 (-24) → i64.
test.expect(1000 as i8 as i64, -24)
// Up-then-down: bit-pattern preserved through u8.
test.expect(257 as u8 as i64, 1)
// Negative through u8 — wraps via two's complement.
test.expect(-1 as u8 as i64, 255)
test.expect(-1 as u16 as i64, 65535)
// Round-trip through f64.
test.expect(123 as f64 as i64, 123)
// Float fractional truncates toward zero in both directions.
test.expect(0.999 as i64, 0)
test.expect(-0.999 as i64, 0)
test.expect(2.5 as i64, 2)
test.expect(-2.5 as i64, -2)
test.expect(2.999 as i64, 2)
test.expect(-2.999 as i64, -2)

// f32 → f64 → i64 chain.
let x: f32 = 7.7_f32
test.expect(x as f64 as i64, 7)

// Repeated narrowing pins down direction (each step truncates).
test.expect(70000 as u16 as i64, 4464)        // 70000 mod 65536
test.expect(70000 as u16 as u8 as i64, 112)   // 4464 mod 256
