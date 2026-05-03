// Class-level mutable storage shared by all instances.
use test

class Counter {
    n: i64
    init() { this.n = 0 }
    bump() { this.n = this.n + 1; Counter.total = Counter.total + 1 }

    static total: i64 = 0
    static threshold: i64 = 1 + 2 * 5      // const-folded: 11
    static enabled: bool = !(1 == 2)
    static factor: f64 = 1.5
}

test.expect(Counter.total, 0)
test.expect(Counter.threshold, 11)
test.expectBool(Counter.enabled, true)
test.expectF64(Counter.factor, 1.5)

let a = new Counter()
let b = new Counter()
a.bump(); a.bump(); b.bump()
test.expect(a.n, 2)
test.expect(b.n, 1)
test.expect(Counter.total, 3)         // shared across instances

// Direct write
Counter.threshold = 100
test.expect(Counter.threshold, 100)

Counter.factor = Counter.factor * 2.0
test.expectF64(Counter.factor, 3.0)
