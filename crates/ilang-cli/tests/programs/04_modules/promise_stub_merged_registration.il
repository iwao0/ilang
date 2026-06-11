// Release builds merge identical Rust functions (ICF):
// `promise_resolve_stub` / `promise_race_resolve_stub` (and the
// reject pair) have identical bodies and collapse to one address.
// The closure capture table keys off fn_addr, so `Promise.all` /
// `Promise.race` registering their stubs after the executor stubs
// used to append a DUPLICATE (+16, KIND_PROMISE) entry to the
// merged address — and every executor-promise callback cell freed
// afterwards released its captured promise twice. The promise died
// early, drained its waiters, and a later timer-driven reject hit
// a dead promise: `.catch` silently never fired (or the program
// aborted on the reused memory).
//
// Order matters here: the aggregates run FIRST to poison the
// registration, then the deferred promise is constructed.
//
// expect: race: r
// expect: all: 11
// expect: caught: late-boom

use std.time as time
use std.test as test

let _all = Promise.all([Promise.resolve(5), Promise.resolve(6)]).then(fn(vs: i64[]) {
    test.expect(vs[0] + vs[1], 11)
    console.log("all: " + (vs[0] + vs[1]).toString())
})
let _race = Promise.race([Promise.resolve("r")]).then(fn(v: string) {
    console.log("race: " + v)
})

let sr: fn(string)[] = []
let pr = new Promise<i64>(fn(res: fn(i64), rej: fn(string)) { sr.push(rej) })
let _t = time.setTimeout(fn() {
    let j = sr[0]
    j("late-boom")
}, 5)
let _c = pr.catch(fn(msg: string): i64 {
    console.log("caught: " + msg)
    return -1
})
