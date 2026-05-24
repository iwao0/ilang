// Two classes calling each other's methods via stored references.
// `A.bump` delegates to `B.dec`, which recursively decrements until
// it reaches 1. Exercises method-dispatch through a heap field plus
// the inner class's self-recursion in the same trace.
use std.test

class A {
    counter: B
    init(b: B) { this.counter = b }
    bump(n: i64) {
        if n > 0 { this.counter.dec(n) }
    }
}

class B {
    log: i64[]
    init(l: i64[]) { this.log = l }
    dec(n: i64) {
        this.log.push(n)
        if n > 1 { this.dec(n - 1) }
    }
}

let log: i64[] = []
let b = new B(log)
let a = new A(b)
a.bump(5)
test.expect(log.length, 5)
test.expect(log[0], 5)
test.expect(log[4], 1)

// Sum
let total = 0
for v in log { total += v }
test.expect(total, 15)

// Re-entry from a fresh `bump` continues to work.
a.bump(3)
test.expect(log.length, 8)
test.expect(log[5], 3)
test.expect(log[7], 1)
