// Fresh heap args passed to a CLOSURE-typed callee follow the same
// transfer rule as named-fn calls: params are borrows, so the
// caller's +1 must be released after the call. The indirect-call
// lowering was missing that release — every fresh string / object
// handed to a closure leaked one reference per call (the Promise
// executor's resolve/reject were the visible case: a resolved heap
// value leaked once per promise).
//
// expect: closure-arg delta=0 strings=0
// expect: sums ok
// expect: promise-value delta=0

use std.time as time
use std.test as test

class Box10 { n: i64; init(x: i64) { this.n = x } }

// The callbacks accumulate into outer cells; assertions happen
// OUTSIDE the measured loop. Both probe values are read into
// locals BEFORE building the report string — a concat evaluates
// left to right, so reading `liveStringCount()` mid-expression
// would see the already-built left half as a live temporary and
// report a phantom +1.
let sums: i64[] = [0, 0]
let eatS: fn(string) = fn(s: string) { sums[0] = sums[0] + s.length }
let eatB: fn(Box10) = fn(b: Box10) { sums[1] = sums[1] + b.n }

let sbase = test.liveStringCount()
let base = test.liveAllocBytes()
let i = 0
while i < 200 {
    eatS("x" + i.toString())
    eatB(new Box10(i))
    i = i + 1
}
let d_bytes = test.liveAllocBytes() - base
let d_strings = test.liveStringCount() - sbase
console.log("closure-arg delta=" + d_bytes.toString()
    + " strings=" + d_strings.toString())
test.expectTrue(sums[0] >= 200)
test.expectTrue(sums[1] > 0)
console.log("sums ok")

// The promise-side manifestation: a fresh heap value resolved via
// the executor's `resolve` closure, consumed by 3 waiters.
let base2 = test.liveAllocBytes()
let k = 0
while k < 200 {
    let p = new Promise<Box10>(fn(res: fn(Box10), rej: fn(string)) {
        res(new Box10(k))
    })
    let _ = p.then(fn(b: Box10) { test.expect(b.n, k) })
    let _ = p.then(fn(b: Box10) { test.expect(b.n, k) })
    let _ = p.then(fn(b: Box10) { test.expect(b.n, k) })
    time.tick()
    k = k + 1
}
console.log("promise-value delta=" + (test.liveAllocBytes() - base2).toString())
