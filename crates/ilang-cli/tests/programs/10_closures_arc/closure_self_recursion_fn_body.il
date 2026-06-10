// Self-recursive closures inside fn bodies. The body's reference to
// its own binding name resolves to the running closure itself
// (Inst::ClosureSelf — the hidden env param), not a capture: a
// capture would snapshot a not-yet-built value or retain-cycle
// through a cell. Requires the explicit fn-type annotation (the
// recursive type can't be inferred). Top-level self-recursion is
// pinned by closure_self_recursion.il (slot late-binding).

use std.test as test

// plain fn-body self-recursion
fn host(): i64 {
    let fib: fn(i64): i64 = fn(n: i64): i64 {
        if n < 2 { n } else { fib(n - 1) + fib(n - 2) }
    }
    fib(10)
}
test.expect(host(), 55)

// self-ref alongside ordinary captures
fn hostCap(bias: i64): i64 {
    let down: fn(i64): i64 = fn(n: i64): i64 {
        if n <= 0 { bias } else { down(n - 1) + 1 }
    }
    down(5)
}
test.expect(hostCap(100), 105)

// the closure escapes its defining scope — the self-reference
// travels with the closure (it IS the closure), no dangling
fn make(): fn(i64): i64 {
    let f: fn(i64): i64 = fn(n: i64): i64 {
        if n <= 0 { 0 } else { n + f(n - 1) }
    }
    f
}
let esc = make()
test.expect(esc(4), 10)

// a nested closure referencing the ENCLOSING closure's name —
// captured as a value snapshot of ClosureSelf (real ownership,
// not a cycle: the enclosing closure doesn't hold the nested one)
fn hostNested(): i64 {
    let f: fn(i64): i64 = fn(n: i64): i64 {
        if n <= 0 { 0 } else {
            let g = fn(): i64 { f(n - 1) }
            n + g()
        }
    }
    f(4)
}
test.expect(hostNested(), 10)

// churn: closures + envs balance
let base = test.liveAllocBytes()
let i = 0
while i < 200 {
    test.expect(host(), 55)
    test.expect(hostNested(), 10)
    let g = make()
    test.expect(g(3), 6)
    i = i + 1
}
test.expectTrue(test.liveAllocBytes() - base < 1024)
