// A suspended async fn (one that hits an `await`) that returns a heap
// value used to LEAK that value. The state-machine desugar settles the
// result promise with `Promise.$promise.settleResolve(state_ref.
// __async_promise, <value>)`, and the lowering retained the promise
// arg — but `settle_resolve` only borrows the promise (locks it,
// transitions state, queues continuations) and never releases that
// reference. The unmatched +1 leaked the result promise, so its
// resolved heap value was never reclaimed (one object per call). The
// zero-await path (`Promise.resolve(body)`) was unaffected, which is
// why it went unnoticed.
//
// Here both the box held across the await and the box returned from the
// async fn must deinit exactly once.
//
// expect: ok
use std.time as time
use std.test as test

class Tally { n: i64; init() { this.n = 0 } }
let tally = new Tally()

class Box {
    pub v: i64
    init(v: i64) { this.v = v }
    deinit() { tally.n = tally.n + 1 }
}

async fn build(p: Promise<i64>): Box {
    let kept = new Box(100)   // held across the await — must be reclaimed
    let n: i64 = await p
    new Box(kept.v + n)       // returned — used to leak
}

build(Promise.resolve(23)).then(fn(b: Box) { test.expect(b.v, 123) })

let _ = time.setTimeout(fn() {
    // Box(100) (held across the suspension) and Box(123) (the return
    // value) each deinit exactly once — no leak.
    test.expect(tally.n, 2)
    console.log("ok")
}, 5)
