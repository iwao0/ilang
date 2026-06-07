// Referring to a top-level `fn` as a function value
// (`let f = some_fn`) auto-wraps it in a trampoline closure that
// ignores the env slot and forwards to the target. Per
// syntax.md §6 "Top-level functions/classes ... auto-wraps it in
// a trampoline closure".
use std.test as test

fn double(n: i64): i64 { n * 2 }

fn apply(g: fn(i64): i64, x: i64): i64 { g(x) }

// Bind a top-level fn to a local — type is inferred.
let f = double
test.expect(f(7), 14)

// Pass it through a higher-order function.
test.expect(apply(double, 9), 18)

// Aliasing the binding shouldn't disturb the original — a fresh
// `let` to the same target still produces a callable trampoline.
let g = double
test.expect(g(11), 22)
