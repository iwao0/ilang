// Float → integer cast must saturate per Rust's `as` semantics
// (since Rust 1.45 / `cranelift::ins::fcvt_to_*_sat`):
//   NaN          → 0
//   +Inf         → target type's MAX
//   -Inf         → target type's MIN (signed) / 0 (unsigned)
//   out-of-range → saturated to MAX / MIN
//   in-range     → truncate toward zero
//
// Pre-fix the JIT used the trapping `fcvt_to_sint` form which
// invoked an integer-overflow trap on NaN / Inf / very-large
// floats — would crash the process at the SDL audio init path
// in `examples/sdl_breakout` once `consts.sampleRate as f64 / …`
// produced an out-of-range temporary. (37d3dd3 switched to the
// saturating form and added matching unsigned int handling.)
use test

let nan = 0.0 / 0.0
let pinf = 1.0 / 0.0
let ninf = -1.0 / 0.0
let big_pos = 1.0e30
let big_neg = -1.0e30

// --- f64 → i64 ---
test.expect(nan as i64, 0)
test.expect(pinf as i64, 9223372036854775807)            // i64::MAX
test.expect(ninf as i64, -9223372036854775807 - 1)       // i64::MIN
test.expect(big_pos as i64, 9223372036854775807)
test.expect(big_neg as i64, -9223372036854775807 - 1)

// In-range — truncate toward zero (the unfixed path also
// produces these correctly; included for the contrast).
test.expect(12.5 as i64, 12)
test.expect(-12.5 as i64, -12)
test.expect(0.999 as i64, 0)
test.expect(-0.999 as i64, 0)

// --- f64 → i32 (saturate within i32 range, then sextend to i64
// for assertion). `as i64` widens via sextend so negative i32
// values compare correctly here even though the separate
// `i32.toString()` display path uses uextend.
test.expect((nan as i32) as i64, 0)
test.expect((pinf as i32) as i64, 2147483647)            // i32::MAX
test.expect((ninf as i32) as i64, -2147483648)           // i32::MIN
test.expect((big_pos as i32) as i64, 2147483647)
test.expect((big_neg as i32) as i64, -2147483648)

// --- f64 → u32 ---
test.expect((nan as u32) as i64, 0)
test.expect((pinf as u32) as i64, 4294967295)            // u32::MAX
test.expect((ninf as u32) as i64, 0)
test.expect((big_pos as u32) as i64, 4294967295)
test.expect((big_neg as u32) as i64, 0)
