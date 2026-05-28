// Shared mutable capture (JS / Swift-default): a closure that mutates
// a captured variable writes through to the SAME storage the outer
// scope (and sibling closures) see. Works for both function-local
// `let`s (bound as a shared heap cell) and top-level bindings (the
// shared module slot). Read-only captures keep their value-snapshot.
use std.test as test

// Function-local accumulator: the closure's writes reach the outer.
fn sumLocal(): i64 {
    let acc = 0
    let ns: i64[] = [1, 2, 3, 4]
    ns.forEach(fn(x: i64) { acc = acc + x })
    acc
}
test.expect(sumLocal(), 10)

// A single mutator closure called repeatedly accumulates in the outer.
fn bumpLocal(): i64 {
    let c = 0
    let bump = fn() { c = c + 1 }
    bump()
    bump()
    bump()
    c
}
test.expect(bumpLocal(), 3)

// Read-only capture is still a snapshot: reassigning the outer after
// the closure is built does NOT change what the closure reads.
fn snapshot(): i64 {
    let factor = 10
    let f = fn(x: i64): i64 { x * factor }
    let before = f(2)
    let factor = 999
    before + f(2)
}
test.expect(snapshot(), 40)

// Different invocations get independent cells.
fn makeCounter(): fn(): i64 {
    let n = 0
    fn(): i64 {
        n = n + 1
        n
    }
}
let a = makeCounter()
let b = makeCounter()
test.expect(a(), 1)
test.expect(a(), 2)
test.expect(b(), 1)
test.expect(a(), 3)
