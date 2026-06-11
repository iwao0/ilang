// `Promise.all` / `Promise.race` receive their input array with a
// transferred +1 (the lowering retains a non-fresh arg, a fresh
// literal hands its own reference in — the same convention as the
// executor) and must consume it after registering the upstream
// continuations. Before the fix every call leaked the input array;
// a fresh-literal input also pinned its upstream promises (and
// their settled values) forever.
//
// expect: all-literal delta=0
// expect: race-named delta=0
// expect: partial-reject delta=0
// expect: race-losers delta=0
// expect: never-settled delta=0

use std.time as time
use std.test as test
class Box { n: i64; init(x: i64) { this.n = x } }

let b1 = test.liveAllocBytes()
let i = 0
while i < 200 {
    let _ = Promise.all([Promise.resolve(i)]).then(fn(vs: i64[]) { test.expect(vs[0], i) })
    time.tick()
    i = i + 1
}
let d1 = test.liveAllocBytes() - b1
console.log("all-literal delta=" + d1.toString())

let b2 = test.liveAllocBytes()
let j = 0
while j < 200 {
    let parr: Promise<i64>[] = [Promise.resolve(j)]
    let _ = Promise.race(parr).then(fn(v: i64) { test.expect(v, j) })
    time.tick()
    j = j + 1
}
let d2 = test.liveAllocBytes() - b2
console.log("race-named delta=" + d2.toString())

// resolve 済みが混ざった列の途中 reject — 部分的に集まった
// slot retain は reject stub が解放する
let b3 = test.liveAllocBytes()
let k = 0
while k < 200 {
    let arr: Promise<Box>[] = [
        Promise.resolve(new Box(k)),
        new Promise<Box>(fn(res: fn(Box), rej: fn(string)) { rej("mid" + k.toString()) }),
        Promise.resolve(new Box(k + 1)),
    ]
    let _ = Promise.all(arr).catch(fn(m: string): Box[] {
        let empty: Box[] = []
        return empty
    })
    time.tick()
    k = k + 1
}
let d3 = test.liveAllocBytes() - b3
console.log("partial-reject delta=" + d3.toString())

// race の敗者の解決値は捨てられる
let b4 = test.liveAllocBytes()
let m = 0
while m < 200 {
    let arr2: Promise<Box>[] = [
        Promise.resolve(new Box(m)),
        Promise.resolve(new Box(m + 100)),
    ]
    let _ = Promise.race(arr2).then(fn(b: Box) { test.expect(b.n, m) })
    time.tick()
    m = m + 1
}
let d4 = test.liveAllocBytes() - b4
console.log("race-losers delta=" + d4.toString())

// settle されないまま捨てる all — waiter ごと回収される
let b5 = test.liveAllocBytes()
let n = 0
while n < 200 {
    let s: fn(Box)[] = []
    let pend = new Promise<Box>(fn(res: fn(Box), rej: fn(string)) { s.push(res) })
    let _ = Promise.all([pend]).then(fn(vs: Box[]) { test.expect(0, 1) })
    n = n + 1
}
let d5 = test.liveAllocBytes() - b5
console.log("never-settled delta=" + d5.toString())
