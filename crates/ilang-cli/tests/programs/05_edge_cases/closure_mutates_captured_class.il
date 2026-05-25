// Closure capturing a class instance (heap, by-value capture of the
// reference) and calling its mutating method. The instance reference
// is captured inline (immutable binding — the body never reassigns
// `c`), but the underlying object is shared, so the closure's calls
// mutate the caller's instance.
use std.test as test
class Counter {
    n: i64
    init() { this.n = 0 }
    bump() { this.n = this.n + 1 }
    get(): i64 { this.n }
}

fn makeBumper(c: Counter): fn() {
    fn() { c.bump() }
}

let counter = new Counter()
let bump = makeBumper(counter)
bump()
bump()
bump()
test.expect(counter.get(), 3)

// Two bumpers over the same instance share state through the
// instance — they aren't shielded by closure boundaries because
// the captured Counter reference is the same allocation.
let bump2 = makeBumper(counter)
bump2()
test.expect(counter.get(), 4)
bump()
test.expect(counter.get(), 5)

// A bumper over a fresh instance is independent.
let other = new Counter()
let bump3 = makeBumper(other)
bump3()
bump3()
test.expect(other.get(), 2)
test.expect(counter.get(), 5)
