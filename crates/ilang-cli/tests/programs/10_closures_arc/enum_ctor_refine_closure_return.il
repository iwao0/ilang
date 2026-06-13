// Completes the enum-ctor type-arg refinement (第48 / 第49): a CLOSURE
// body must also refine a generic enum ctor in its tail / early returns
// from the closure's declared return type, the same way a top-level fn
// body does. Without it, `fn(): Result<i64,string> { Result.err("e") }`
// (or `array.map(fn -> Result<...>)`) left the unfilled `T` as `Any` and
// the monomorphizer aborted with "Type::Any (variadic builtins)".
//
// Covers the closure tail, an early `return` inside a closure, and a
// `map` callback that builds Results. Heap payloads flow through; ARC
// exact.

use std.test as test

let deinits: i64[] = [0]

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

// Closure tail returns an err (T unfilled). no Box.
fn closureTailErr(): i64 {
    let f: fn(): Result<Box, string> = fn(): Result<Box, string> {
        Result.err("boom")
    }
    match f() {
        ok(b) { b.n }
        err(e) { -1 }
    }
}

// Closure early return err, tail ok with a Box. 1 Box/round on the ok path.
fn closureEarlyReturn(neg: bool): i64 {
    let f: fn(bool): Result<Box, string> = fn(x: bool): Result<Box, string> {
        if x { return Result.err("neg") }
        Result.ok(new Box(7))
    }
    match f(neg) {
        ok(b) { b.n }
        err(e) { -1 }
    }
}

// map callback building Results; ok carries a Box. 2 Boxes/round.
fn mapToResults(): i64 {
    let xs: i64[] = [1, 9, 3]
    let rs: Result<Box, string>[] = xs.map(fn(x: i64): Result<Box, string> {
        if x < 5 { Result.ok(new Box(x)) } else { Result.err("big") }
    })
    let sum = 0
    for r in rs {
        sum = sum + match r {
            ok(b) { b.n }
            err(e) { 0 }
        }
    }
    sum                                  // 1 + 0 + 3 = 4
}

let acc: i64[] = [0]

test.expect(closureTailErr(), -1)
test.expect(closureEarlyReturn(true), -1)    // early return err, no Box
test.expect(closureEarlyReturn(false), 7)    // ok path, 1 Box
test.expect(mapToResults(), 4)

// Per round: closureEarlyReturn(false) 1 Box + mapToResults 2 Boxes = 3.
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    acc[0] = acc[0] + closureTailErr() + closureEarlyReturn(true)
        + closureEarlyReturn(false) + mapToResults()
    i = i + 1
}
test.expect(deinits[0] - bd, 300)
test.expect(test.liveAllocBytes() - d0, 0)
