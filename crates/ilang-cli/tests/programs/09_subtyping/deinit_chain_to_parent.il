// Swift-style deinit chaining: when an instance dies, the most
// derived class's deinit runs first, then each ancestor's deinit in
// turn — automatically, without the user calling super. Classes
// without their own deinit are skipped but don't break the chain,
// and an early `return` inside a deinit body still chains (the chain
// lives in a synthesized wrapper, not a tail-appended call).

use std.test as test

class Counter {
    n: i64
    init() { this.n = 0 }
    bump(amount: i64) { this.n = this.n + amount }
}

class A {
    c: Counter
    init(c: Counter) { this.c = c }
    deinit() { this.c.bump(1) }
}

// B has no deinit of its own — the chain passes through to A.
class B: A {
    init(c: Counter) { super(c) }
}

class C: B {
    extra: Counter
    init(c: Counter) {
        super(c)
        this.extra = c
    }
    deinit() { this.extra.bump(100) }
}

// Early return must not skip the ancestor chain.
class D: A {
    flag: i64
    init(c: Counter) {
        super(c)
        this.flag = 1
    }
    deinit() {
        if flag == 1 { this.c.bump(10); return }
        this.c.bump(20)
    }
}

let counter = new Counter()

// Two-level chain through a deinit-less middle class, released via a
// base-typed binding (dynamic class_id dispatch picks C's chain).
fn dropC(c: Counter) {
    let b: A = new C(c)
    let _ = b
}
dropC(counter)
test.expect(counter.n, 101)   // C's 100, then A's 1

// Early-return body still chains to A.
fn dropD(c: Counter) {
    let d = new D(c)
    let _ = d
}
dropD(counter)
test.expect(counter.n, 112)   // +10 (D early path) +1 (A)

// Churn: chained deinits fire exactly once per instance, no leak.
let base = test.liveAllocBytes()
let before = counter.n
let i = 0
while i < 200 {
    dropC(counter)
    i = i + 1
}
test.expect(counter.n, before + 200 * 101)
let after = test.liveAllocBytes()
test.expectTrue(after - base < 1024)
