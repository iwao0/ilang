// Float division by zero does NOT panic — it follows IEEE 754:
// finite / 0.0 → inf / -inf, 0.0 / 0.0 → NaN.
use std.test

let inf = 1.0 / 0.0
let neg_inf = -1.0 / 0.0
let nan = 0.0 / 0.0

// inf is greater than every finite number.
test.expectBool(inf > 1.0e300, true)
test.expectBool(neg_inf < -1.0e300, true)

// NaN compares unequal to everything, including itself.
test.expectBool(nan == nan, false)
test.expectBool(nan != nan, true)

// 1/0 patterns at higher widths.
let f32_inf: f32 = 1.0_f32 / 0.0_f32
test.expectBool((f32_inf as f64) > 1.0e30, true)
