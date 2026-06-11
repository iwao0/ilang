// ARC of the three cells `new Promise<T>(executor)` mints: the
// executor closure arrives with a transferred +1 and the runtime
// allocates resolve_cb / reject_cb with rc=1 — all three must be
// released by `__promise_with_executor` after the synchronous
// executor call. Before the fix every `new Promise` leaked exactly
// 16 + 32 + 24 bytes (plus captures) regardless of what the
// executor did. An executor that stores a callback into a container
// (the deferred pattern, probed in variant c) keeps it alive via
// the container's own retain.
//
// expect: empty delta=0
// expect: inline-resolve delta=0
// expect: escape-no-call delta=0
// expect: escape-call delta=0

use std.time as time
use std.test as test

let base_a = test.liveAllocBytes()
let a = 0
while a < 200 {
    let _p = new Promise<i64>(fn(resolve: fn(i64), reject: fn(string)) {})
    a = a + 1
}
console.log("empty delta=" + (test.liveAllocBytes() - base_a).toString())

let base_b = test.liveAllocBytes()
let b = 0
while b < 200 {
    let p = new Promise<i64>(fn(resolve: fn(i64), reject: fn(string)) { resolve(b) })
    let _ = p.then(fn(v: i64) { test.expect(v, b) })
    time.tick()
    b = b + 1
}
console.log("inline-resolve delta=" + (test.liveAllocBytes() - base_b).toString())

let base_c = test.liveAllocBytes()
let c = 0
while c < 200 {
    let saved: fn(i64)[] = []
    let _p = new Promise<i64>(fn(resolve: fn(i64), reject: fn(string)) {
        saved.push(resolve)
    })
    c = c + 1
}
console.log("escape-no-call delta=" + (test.liveAllocBytes() - base_c).toString())

let base_d = test.liveAllocBytes()
let d = 0
while d < 200 {
    let saved: fn(i64)[] = []
    let p = new Promise<i64>(fn(resolve: fn(i64), reject: fn(string)) {
        saved.push(resolve)
    })
    let _ = p.then(fn(v: i64) { test.expect(v, d) })
    let r = saved[0]
    r(d)
    time.tick()
    d = d + 1
}
console.log("escape-call delta=" + (test.liveAllocBytes() - base_d).toString())
