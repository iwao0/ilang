// Recursive function — classic fib. Verifies the call stack handles
// depth ≥ 20 in both interp and JIT.
use std.test as test
fn fib(n: i64): i64 {
    if n < 2 { return n }
    fib(n - 1) + fib(n - 2)
}

test.expect(fib(0), 0)
test.expect(fib(1), 1)
test.expect(fib(10), 55)
test.expect(fib(20), 6765)
test.expect(fib(25), 75025)

// Mutual recursion.
fn is_even(n: i64): bool {
    if n == 0 { return true }
    is_odd(n - 1)
}
fn is_odd(n: i64): bool {
    if n == 0 { return false }
    is_even(n - 1)
}
test.expectBool(is_even(20), true)
test.expectBool(is_odd(21), true)
test.expectBool(is_even(15), false)
