// A self-recursive closure (`let fib = fn() { ... fib(...) ... }`)
// is called repeatedly. Each call's stack frame allocates closure
// arg slots and intermediate i64 values, but the closure value
// itself lives in the global slot — the body's `fib` lookup
// doesn't allocate per call. A regression that re-allocated the
// closure on every body's `fib` reference would leak ~24 bytes
// per recursive call. Pairs with closure_self_recursion.il
// (functional behaviour); this fixture pins down the no-leak
// guarantee for the common recursive shape.
//

use test

let fib: fn(i64): i64 = fn(n: i64): i64 {
    if n < 2 { n } else { fib(n - 1) + fib(n - 2) }
}

let baseline = test.liveAllocBytes()

let i: i64 = 0
while i < 50 {
    let _ = fib(8)         // 67 calls per fib(8)
    i = i + 1
}

let after = test.liveAllocBytes()
test.expectTrue(after - baseline < 32768)
