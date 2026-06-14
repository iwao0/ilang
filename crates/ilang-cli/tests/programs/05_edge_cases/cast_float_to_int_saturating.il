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
use std.test as test
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

// --- f64 → i8 / i16 / u8 / u16 (narrow targets) ---
// The saturating convert can only target I32/I64 on x64, so the narrow
// cases convert to I32 first. That clamps at the I32 boundary, so the
// I32 result must be re-clamped to the destination type's range before
// the final truncation — otherwise `ireduce` takes the low bits and
// WRAPS (the pre-fix bug: 300.0 as u8 → 44, 40000.0 as i16 → -25536,
// -200.0 as i8 → 56).
test.expect((nan as u8) as i64, 0)
test.expect((pinf as u8) as i64, 255)                    // u8::MAX
test.expect((ninf as u8) as i64, 0)
test.expect((300.0 as u8) as i64, 255)                   // overflow saturates
test.expect((250.0 as u8) as i64, 250)                   // in-range
test.expect((-5.0 as u8) as i64, 0)                      // negative → 0

test.expect((pinf as u16) as i64, 65535)                 // u16::MAX
test.expect((70000.0 as u16) as i64, 65535)              // overflow saturates
test.expect((1000.0 as u16) as i64, 1000)                // in-range

test.expect((pinf as i8) as i64, 127)                    // i8::MAX
test.expect((ninf as i8) as i64, -128)                   // i8::MIN
test.expect((nan as i8) as i64, 0)
test.expect((200.0 as i8) as i64, 127)                   // overflow → MAX
test.expect((-200.0 as i8) as i64, -128)                 // underflow → MIN
test.expect((-100.0 as i8) as i64, -100)                 // in-range negative
test.expect((100.0 as i8) as i64, 100)                   // in-range

test.expect((pinf as i16) as i64, 32767)                 // i16::MAX
test.expect((ninf as i16) as i64, -32768)                // i16::MIN
test.expect((40000.0 as i16) as i64, 32767)              // overflow → MAX
test.expect((-40000.0 as i16) as i64, -32768)            // underflow → MIN
test.expect((-1000.0 as i16) as i64, -1000)              // in-range negative
