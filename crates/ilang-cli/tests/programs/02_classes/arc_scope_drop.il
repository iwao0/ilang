// `deinit` must fire when an object's last strong reference goes out
// of scope. Counted via a shared Counter the tracked objects bump in
// their deinit.
use std.test

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

let total = new Counter()

// Inner block — both objects die at `}`.
{
    let t1 = new Tracked(total)
    let t2 = new Tracked(total)
    test.expect(total.get(), 0)
}
test.expect(total.get(), 2)

// Single object in its own scope.
{
    let _t = new Tracked(total)
}
test.expect(total.get(), 3)

// Disposable as a side effect of an expression statement.
new Tracked(total)
test.expect(total.get(), 4)
