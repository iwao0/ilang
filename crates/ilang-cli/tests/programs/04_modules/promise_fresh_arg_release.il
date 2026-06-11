// A FRESH promise passed as an argument follows the same
// fresh-transfer rule as every other heap type: the callee's
// store (field assignment / container push) takes its own retain,
// so the caller's +1 is released after the call. The per-site
// release lists all omitted `Promise` — a fresh promise handed to
// a `new Class(...)` init (or any fn) leaked together with its
// settled value. The lists are now one shared predicate
// (`fresh_arg_needs_post_release`), pinned here through the init
// shape (deinit count proves the holders themselves still die).
//
// expect: init-arg delta=0 deinits=200
// expect: fn-arg delta=0

use std.time as time
use std.test as test
class Box { n: i64; init(x: i64) { this.n = x } }

let deinits: i64[] = [0]
class Holder {
    p: Promise<Box>
    init(q: Promise<Box>) { this.p = q }
    deinit() { deinits[0] = deinits[0] + 1 }
}

let b1 = test.liveAllocBytes()
let i = 0
while i < 200 {
    let h = new Holder(Promise.resolve(new Box(i)))
    let _ = h.p.then(fn(b: Box) { test.expect(b.n, i) })
    time.tick()
    i = i + 1
}
let d1 = test.liveAllocBytes() - b1
console.log("init-arg delta=" + d1.toString() + " deinits=" + deinits[0].toString())

fn observe(q: Promise<Box>, expect_n: i64) {
    let _ = q.then(fn(b: Box) { test.expect(b.n, expect_n) })
}
let b2 = test.liveAllocBytes()
let j = 0
while j < 200 {
    observe(Promise.resolve(new Box(j)), j)
    time.tick()
    j = j + 1
}
let d2 = test.liveAllocBytes() - b2
console.log("fn-arg delta=" + d2.toString())
