// Bare references to top-level fns reify as `fn(...): T` values:
// they can be passed as arguments, bound to locals, and reassigned.
// Pre-fix the JIT generated a per-name trampoline lazily; this
// fixture exercises the round-trip through both backends.
use std.test as test
fn double(x: i64): i64 { x * 2 }
fn triple(x: i64): i64 { x * 3 }
fn negate(x: i64): i64 { -x }

fn apply(f: fn(i64): i64, x: i64): i64 { f(x) }
test.expect(apply(double, 7), 14)
test.expect(apply(triple, 7), 21)
test.expect(apply(negate, 7), -7)

// Bound to a local.
let f = double
test.expect(f(10), 20)

// Reassign to a different top-level fn.
let f = triple
test.expect(f(10), 30)

// Pass via a chain.
fn applyTwice(f: fn(i64): i64, x: i64): i64 { f(f(x)) }
test.expect(applyTwice(double, 3), 12)             // 3 → 6 → 12
test.expect(applyTwice(triple, 2), 18)             // 2 → 6 → 18
test.expect(applyTwice(negate, 5), 5)              // -5 → 5
