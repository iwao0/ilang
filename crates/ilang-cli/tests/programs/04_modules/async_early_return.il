// Early `return` inside an `async fn` body. The desugar rewrites
// every copied `return e` into "settle the result promise with `e`,
// then a bare `return`" — the poll fn returns `()`, so a raw user
// `return` used to fail with a misleading type error ("expected (),
// got i64") and the shape was simply unsupported. Covers: return
// before the first await, return between two awaits (both taken and
// not-taken paths), and the leak-free churn of the taken path.
//
// expect: pre-await: 6
// expect: post-await: 18
// expect: between-return: 9
// expect: between-through: 5
// expect: churn ok strings=0

use std.time as time
use std.test as test

async fn maybeAwait(flag: bool, p: Promise<i64>): i64 {
    let tag: string = "tag" + flag.toString()
    if flag { return tag.length - 1 }
    let v = await p
    v + tag.length
}
let _ = maybeAwait(true, Promise.resolve(1)).then(fn(v: i64) {
    console.log("pre-await: " + v.toString())
})
let _ = maybeAwait(false, Promise.resolve(10)).then(fn(v: i64) {
    console.log("post-await: " + v.toString())
})
time.tick()

async fn betweenAwaits(a: Promise<i64>, b: Promise<i64>): i64 {
    let x = await a
    if x > 5 { return x }
    let y = await b
    x + y
}
let _ = betweenAwaits(Promise.resolve(9), Promise.resolve(1)).then(fn(v: i64) {
    console.log("between-return: " + v.toString())
})
let _ = betweenAwaits(Promise.resolve(2), Promise.resolve(3)).then(fn(v: i64) {
    console.log("between-through: " + v.toString())
})
time.tick()

let b1 = test.liveAllocBytes()
let s1 = test.liveStringCount()
let acc: i64[] = [0]
let k = 0
while k < 200 {
    let _ = maybeAwait(true, Promise.resolve(k)).then(fn(v: i64) { acc[0] = acc[0] + v })
    time.tick()
    k = k + 1
}
let db = test.liveAllocBytes() - b1
let ds = test.liveStringCount() - s1
test.expectTrue(db < 1024)
test.expectTrue(acc[0] > 0)
console.log("churn ok strings=" + ds.toString())
