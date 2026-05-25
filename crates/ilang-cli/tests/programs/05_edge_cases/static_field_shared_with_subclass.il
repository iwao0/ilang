// `static_fields.il` covers reading/writing a static field on the
// declaring class. This fixture pins down the next layer: a subclass
// inherits the SAME backing storage — bumps from a `Sub` instance
// land in `Counter.total`, not a per-class shadow. Easy to regress
// during vtable-layout changes.
use std.test as test
class Counter {
    n: i64
    init() { this.n = 0 }
    bump() {
        this.n = this.n + 1
        Counter.total = Counter.total + 1
    }
    static total: i64 = 0
}

class Sub: Counter {
    init() { super() }
}

class GrandSub: Sub {
    init() { super() }
}

let s = new Sub()
let gs = new GrandSub()
let c = new Counter()

s.bump()
s.bump()
gs.bump()
c.bump()

test.expect(Counter.total, 4)        // shared across the whole hierarchy

// Direct write to the static through the parent name is visible
// to instances of any subclass that subsequently bumps.
Counter.total = 100
s.bump()
test.expect(Counter.total, 101)
