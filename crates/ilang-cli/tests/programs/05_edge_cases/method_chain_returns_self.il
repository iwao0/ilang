// Builder-pattern chain: each method returns `this` for fluent calls,
// and the side effect on `this.n` must accumulate exactly once per
// hop. A regression that re-evaluates the receiver between chain
// links (or boxes/unboxes it) would either lose state or undercount.
use test

class Counter {
    n: i64
    init() { this.n = 0 }
    inc(): Counter { this.n = this.n + 1; this }
    add(k: i64): Counter { this.n = this.n + k; this }
    val(): i64 { this.n }
}

let c = new Counter()
test.expect(c.inc().inc().inc().val(), 3)
test.expect(c.inc().inc().val(), 5)
test.expect(c.add(10).add(20).val(), 35)

// Long chain — each call mutates the same instance.
test.expect(
    c.inc().inc().inc().inc().inc().inc().inc().inc().inc().inc().val(),
    45
)

// Chain returning self also works when the value is captured.
let same = c.inc()                    // also returns c
same.inc()
test.expect(c.val(), 47)              // both bumps applied to c
test.expectBool(c == same, true)      // identity preserved across return
