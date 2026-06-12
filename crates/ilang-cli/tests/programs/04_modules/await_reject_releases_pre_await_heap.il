// Confirmation pin (第38弾): two async ARC properties around the
// rejection path, complementing the exit-drain fix in 第37弾.
//
//   1. When `await p` REJECTS, the async fn abandons the rest of its
//      body — but a heap local allocated BEFORE the await must still be
//      released (the same early-exit sweep that covers `return` /
//      `break` through a live heap binding). Without it, every rejected
//      call leaks the pre-await `guard`.
//
//   2. The exit-time event-loop drain (第37弾) must also flush a pending
//      *rejection* whose cleanup touches a top-level global. A rejecting
//      promise left un-ticked at exit drains during `__main`'s epilogue,
//      while the globals are still alive; reaching a clean exit (rather
//      than an out-of-bounds abort) is the guard.
//
// The leak dimension is checked with explicit `time.tick()` so the
// guard-Box deinits are observable; the shutdown-drain dimension is the
// final un-ticked rejection.
//
// expect: ticked deinits=3 catches=3
// expect: left a rejecting promise pending for shutdown drain

use std.test as test
use std.time as time

let deinits: i64[] = [0]
let log: i64[] = [0]

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

async fn risky(pr: Promise<i64>): i64 {
    let guard = new Box(99)       // allocated before the await
    let v: i64 = await pr         // rejects -> body below never runs
    guard.n + v
}

fn fire() {
    let pr: Promise<i64> = new Promise<i64>(fn(res: fn(i64), rej: fn(string)) {
        rej("boom")
    })
    let _ = risky(pr).catch(fn(_m: string): i64 {
        log[0] = log[0] + 1
        return -1
    })
}

// Part 1 — drain each rejection explicitly. 3 calls -> 3 guard Boxes
// released, 3 catches fire.
let i = 0
while i < 3 {
    fire()
    time.tick()
    i = i + 1
}
console.log("ticked deinits=" + deinits[0].toString()
    + " catches=" + log[0].toString())

// Part 2 — leave a rejecting promise un-ticked. Its guard Box is freed
// only by the shutdown drain; that drain must run before `deinits` is
// torn down, or `deinit` aborts on freed memory.
fire()
console.log("left a rejecting promise pending for shutdown drain")
