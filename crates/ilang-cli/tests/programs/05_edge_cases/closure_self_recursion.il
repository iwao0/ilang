// `let f: fn(...) = fn(...) { ... f(...) ... }` — a closure whose
// body calls itself by name. The capture mechanism can't snapshot
// `f` at construction time (the closure value isn't built yet), so
// the typechecker drops the self-reference from the FnExpr's capture
// list and codegen resolves the name at call time through the
// top-level `let` slot. Pre-fix the JIT errored at lower-time with
// "unknown function 'f'"; the interpreter happened to work because
// `f` fell through to the global-let map.
use std.test

let fib: fn(i64): i64 = fn(n: i64): i64 {
    if n < 2 { n } else { fib(n - 1) + fib(n - 2) }
}
test.expect(fib(0), 0)
test.expect(fib(1), 1)
test.expect(fib(2), 1)
test.expect(fib(10), 55)

// Single recursion site in one closure body.
let countdown: fn(i64): i64 = fn(n: i64): i64 {
    if n <= 0 { 0 } else { 1 + countdown(n - 1) }
}
test.expect(countdown(0), 0)
test.expect(countdown(5), 5)
test.expect(countdown(8), 8)

// A closure that calls itself AND another top-level closure. The
// non-self reference is captured normally; the self-reference is
// resolved through the global slot. (`countdown` was rebound in
// previous let — the re-bound version is what `useTwo` sees, since
// we only drop the let's OWN name from its captures.)
let helper: fn(i64): i64 = fn(n: i64): i64 { n * 2 }
let useTwo: fn(i64): i64 = fn(n: i64): i64 {
    if n <= 0 { 0 } else { helper(n) + useTwo(n - 1) }
}
test.expect(useTwo(0), 0)
test.expect(useTwo(1), 2)
test.expect(useTwo(3), 12)            // 6 + 4 + 2 + 0
