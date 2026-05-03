// Read-only computed property (getter, no setter).
use test

class Counter {
    n: i64
    init() { this.n = 0 }
    bump() { this.n = this.n + 1 }
    get value(): i64 { this.n }
    get doubled(): i64 { this.n * 2 }
}

let c = new Counter()
test.expect(c.value, 0)
test.expect(c.doubled, 0)
c.bump()
c.bump()
c.bump()
test.expect(c.value, 3)
test.expect(c.doubled, 6)
