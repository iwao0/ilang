// `?` on a heap-payload `Result` must keep ARC exact on BOTH paths: the
// ok path carries the `Box` out, and the err path early-returns
// `Result.err(e)` — its synthesised return jumps over any live heap
// binding above it, which the early-exit sweep has to release.
// `try_operator.il` pins the functional behaviour on `i32`; this pins
// the heap/ARC side (a live `Box` crossed by the `?`-err exit drops
// exactly once, no leak, no over-release). A clean-round fixture — no
// bug, the form just had no coverage.

// expect: a=106 b=-1 deinits=3
// expect: churn delta=0 deinits=300

use std.test as test

let deinits: i64[] = [0]

class Box {
    pub n: i64
    init(n: i64) { this.n = n }
    deinit() { deinits[0] = deinits[0] + 1 }
}

fn fetch(ok: bool): Result<Box, string> {
    if ok { Result.ok(new Box(7)) } else { Result.err("fail") }
}

fn process(ok: bool): Result<i64, string> {
    let live = new Box(99)        // live across the `?`
    let got = fetch(ok)?          // err path early-returns over `live`
    Result.ok(got.n + live.n)
}

fn run(ok: bool): i64 {
    match process(ok) {
        ok(v) { v }
        err(_) { -1 }
    }
}

let bd = deinits[0]
let a = run(true)                 // 7 + 99
let b = run(false)                // err -> -1
// run(true): fetch's Box(7) + live Box(99) = 2; run(false): live Box(99)
// (fetch errored, no Box) = 1. Total 3.
test.expect(a, 106)
test.expect(b, -1)
test.expect(deinits[0] - bd, 3)
console.log("a=106 b=-1 deinits=3")

let acc: i64[] = [0]
let d0 = test.liveAllocBytes()
let bd1 = deinits[0]
let i = 0
while i < 100 { acc[0] = acc[0] + run(true) + run(false); i = i + 1 }
test.expect(test.liveAllocBytes() - d0, 0)
test.expect(deinits[0] - bd1, 300)   // 3 boxes per round, exactly
console.log("churn delta=0 deinits=300")
