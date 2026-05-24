// Float special-value arithmetic. IEEE 754 says inf-inf, inf*0,
// inf+(-inf) all produce NaN; inf+inf stays inf; finite/inf is +0.
// Negative zero compares equal to positive zero.
use std.test

let inf = 1.0 / 0.0
let neg_inf = -1.0 / 0.0

let r1 = inf - inf
test.expectBool(r1 == r1, false)         // NaN

let r2 = inf * 0.0
test.expectBool(r2 == r2, false)         // NaN

let r3 = inf + neg_inf
test.expectBool(r3 == r3, false)         // NaN

test.expectBool((inf + inf) == inf, true)
test.expectBool((neg_inf + neg_inf) == neg_inf, true)

test.expectF64(0.0 / inf, 0.0)
test.expectF64(1.0 / inf, 0.0)
test.expectF64(-1.0 / inf, 0.0)          // negative zero, but ==0.0

test.expectBool(-0.0 == 0.0, true)
test.expectBool(0.0 == -0.0, true)
