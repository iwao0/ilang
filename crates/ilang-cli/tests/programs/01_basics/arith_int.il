// Integer arithmetic + precedence + parens + unary ops.
use test

// Basic four ops + modulo.
test.expect(1 + 2, 3)
test.expect(10 - 3, 7)
test.expect(6 * 7, 42)
test.expect(20 / 4, 5)
test.expect(17 % 5, 2)

// Precedence: * / % bind tighter than + -.
test.expect(1 + 2 * 3, 7)
test.expect(2 * 3 + 4 * 5, 26)
test.expect(20 - 6 / 3, 18)

// Parens override.
test.expect((1 + 2) * 3, 9)
test.expect(2 * (3 + 4), 14)

// Unary - and + (the latter is a no-op).
test.expect(-5 + 10, 5)
test.expect(+7, 7)
test.expect(-(3 + 4), -7)

// Integer division truncates toward zero.
test.expect(7 / 2, 3)
test.expect(-7 / 2, -3)

// Modulo follows the truncated-division sign.
test.expect(7 % 3, 1)
test.expect(-7 % 3, -1)
