// std.math edge cases:
//   * `sign(±0.0)` must be `0.0` (JS `Math.sign`), not `1.0`. The
//     intrinsic mapped to Rust's `f64::signum`, which returns `±1.0`
//     for `±0.0` — contradicting the documented `0.0`.
//   * `min` / `max` must propagate NaN from EITHER side (commutative).
//     The bare `a < b` form returned `a` for a NaN `a` (so `min(nan, x)`
//     wrongly gave `x`) while propagating a NaN `b` — order-dependent.
//
// expect: sign0=0.0
// expect: signNeg0=0.0
// expect: sign5=1.0
// expect: signNeg5=-1.0
// expect: signNaN=NaN
// expect: minNaNFirst=NaN
// expect: minNaNSecond=NaN
// expect: maxNaNFirst=NaN
// expect: maxNaNSecond=NaN
// expect: min37=3.0
// expect: max37=7.0

use std.math as m

let nan = 0.0 / 0.0

console.log("sign0=" + m.sign(0.0).toString())
console.log("signNeg0=" + m.sign(-0.0).toString())
console.log("sign5=" + m.sign(5.0).toString())
console.log("signNeg5=" + m.sign(-5.0).toString())
console.log("signNaN=" + m.sign(nan).toString())

console.log("minNaNFirst=" + m.min(nan, 5.0).toString())
console.log("minNaNSecond=" + m.min(5.0, nan).toString())
console.log("maxNaNFirst=" + m.max(nan, 5.0).toString())
console.log("maxNaNSecond=" + m.max(5.0, nan).toString())
console.log("min37=" + m.min(3.0, 7.0).toString())
console.log("max37=" + m.max(3.0, 7.0).toString())
