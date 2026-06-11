// The deferred pattern (JS `Promise.withResolvers`): the executor
// stores its resolve / reject callbacks into outer containers and
// returns without settling; timers settle the promise later. Pins
// that an escaped callback cell stays alive (the container's retain
// survives the runtime's own release), that a timer-driven resolve
// reaches `await` / `.then` / `Promise.all`, and that an escaped
// reject reaches `.catch`.
//
// expect: main end
// expect: await-deferred: 42
// expect: all-timer-resolved: [100, 200]
// expect: caught: boom

use std.time as time
use std.test as test

// await on a deferred resolved by a timer
let saved: fn(i64)[] = []
let p = new Promise<i64>(fn(resolve: fn(i64), reject: fn(string)) {
    saved.push(resolve)
})
async fn waiter(q: Promise<i64>): i64 {
    let v = await q
    v * 2
}
let _t = time.setTimeout(fn() {
    let r = saved[0]
    r(21)
}, 3)
let _w = waiter(p).then(fn(v: i64) {
    test.expect(v, 42)
    console.log("await-deferred: " + v.toString())
})

// Promise.all over two deferreds settled by timers in reverse order
let s1: fn(i64)[] = []
let s2: fn(i64)[] = []
let pa = new Promise<i64>(fn(res: fn(i64), rej: fn(string)) { s1.push(res) })
let pb = new Promise<i64>(fn(res: fn(i64), rej: fn(string)) { s2.push(res) })
let _t2 = time.setTimeout(fn() { let r = s2[0]; r(200) }, 1)
let _t1 = time.setTimeout(fn() { let r = s1[0]; r(100) }, 6)
let _all = Promise.all([pa, pb]).then(fn(vs: i64[]) {
    test.expect(vs[0], 100)
    test.expect(vs[1], 200)
    console.log("all-timer-resolved: [" + vs[0].toString() + ", " + vs[1].toString() + "]")
})

// escaped reject reaches .catch
let sr: fn(string)[] = []
let pr = new Promise<i64>(fn(res: fn(i64), rej: fn(string)) { sr.push(rej) })
let _t3 = time.setTimeout(fn() {
    let j = sr[0]
    j("boom")
}, 9)
let _c = pr.catch(fn(msg: string): i64 {
    console.log("caught: " + msg)
    return -1
})

console.log("main end")
