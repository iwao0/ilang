// Chained `p.then(f).catch(g)`: the intermediate promise is a
// FRESH method receiver and must be released after the dispatch —
// the upstream's waiter holds +1 on the DOWNSTREAM, so the chain
// stays alive without the caller's reference. Before the fix the
// intermediate ManagedPromise leaked together with its settled
// value (1 rejection string per iteration here; the promise box
// itself is invisible to liveAllocBytes).
//
// expect: chain strings=0
// expect: chain delta=0

use std.time as time
use std.test as test

let sbase = test.liveStringCount()
let base = test.liveAllocBytes()
let i = 0
while i < 200 {
    let p = new Promise<i64>(fn(res: fn(i64), rej: fn(string)) {
        rej("e" + i.toString())
    })
    let _ = p.then(fn(v: i64): i64 { v }).catch(fn(m: string): i64 {
        test.expectTrue(m.length >= 2)
        return -1
    })
    time.tick()
    i = i + 1
}
console.log("chain strings=" + (test.liveStringCount() - sbase).toString())
console.log("chain delta=" + (test.liveAllocBytes() - base).toString())
