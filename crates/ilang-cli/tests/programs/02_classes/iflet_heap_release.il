// Regression: an `if let some(x) = expr` where `x` is heap-typed must
// release `x` at the end of the then-branch and the scrutinee
// allocation itself when the if-let expression finishes. Both sites
// were previously skipped, leaking the +1 retain on `x` and the
// fresh heap value produced by `expr`.
use test

class Counter {
    n: i64
    init() { this.n = 0 }
    bump() { this.n = this.n + 1 }
    get(): i64 { this.n }
}

class Tracked {
    c: Counter
    init(c: Counter) { this.c = c }
    deinit() { this.c.bump() }
}

fn make(c: Counter): Tracked? {
    some(new Tracked(c))
}

let total = new Counter()

// `make(total)` allocates one Tracked. When the if-let finishes,
// the Tracked's last reference must be gone — so deinit runs and
// the counter increments.
let _x = if let some(t) = make(total) { 1 } else { 0 }
test.expect(total.get(), 1)

// Repeat to confirm each invocation drops independently.
let _y = if let some(t) = make(total) { 1 } else { 0 }
let _z = if let some(t) = make(total) { 1 } else { 0 }
test.expect(total.get(), 3)

// `none` branch: scrutinee is a primitive Optional with no inner
// allocation, so nothing to release. Counter stays put.
fn nothing(): Tracked? { none }
let _w = if let some(t) = nothing() { 1 } else { 0 }
test.expect(total.get(), 3)
