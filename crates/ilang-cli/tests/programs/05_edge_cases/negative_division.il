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
