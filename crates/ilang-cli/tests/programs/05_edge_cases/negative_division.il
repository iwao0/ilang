// Integer division truncates toward zero (Rust / C semantics);
// modulo follows the same rule, so `(a/b)*b + a%b == a` always holds.
use std.test as test
test.expect(7 / 2, 3)
test.expect(-7 / 2, -3)
test.expect(7 / -2, -3)
test.expect(-7 / -2, 3)

test.expect(7 % 2, 1)
test.expect(-7 % 2, -1)
test.expect(7 % -2, 1)
test.expect(-7 % -2, -1)

// Identity check: (a / b) * b + a % b == a
let a = -17
let b = 5
test.expect((a / b) * b + a % b, a)

// Overflow corner: i64::MIN / -1 is mathematically 2^63, which doesn't
// fit, so it wraps back to i64::MIN instead of trapping (on x86 a raw
// `idiv` here raises #DE / SIGFPE — codegen must avoid that). Modulo of
// the same pair is 0. Through variables so it runs, not const-folds.
let m: i64 = -9223372036854775808
let neg: i64 = -1
test.expect(m / neg, m)
test.expect(m % neg, 0)
