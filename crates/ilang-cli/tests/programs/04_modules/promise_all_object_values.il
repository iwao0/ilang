// `Promise.all` with HEAP element values: the result array takes
// over the per-slot retains the resolve stubs accumulated —
// `build_i64_array` copies raw slots without retaining, so the old
// "release the slot retains after building" left the elements with
// no owner. That read fine while the input upstreams leaked (their
// Resolved state propped the values up) and became a use-after-free
// (garbage `vs[i].n`, nondeterministic) once `__promise_all`
// started consuming its input array.
//
// expect: values ok
// expect: churn delta=0

use std.time as time
use std.test as test
class Box { n: i64; init(x: i64) { this.n = x } }

let _ = Promise.all([
    Promise.resolve(new Box(10)),
    Promise.resolve(new Box(20)),
    Promise.resolve(new Box(30)),
]).then(fn(vs: Box[]) {
    test.expect(vs.length, 3)
    test.expect(vs[0].n, 10)
    test.expect(vs[1].n, 20)
    test.expect(vs[2].n, 30)
    console.log("values ok")
})
time.tick()

let base = test.liveAllocBytes()
let i = 0
while i < 200 {
    let _ = Promise.all([
        Promise.resolve(new Box(i)),
        Promise.resolve(new Box(i + 1)),
    ]).then(fn(vs: Box[]) {
        test.expect(vs[0].n + vs[1].n, i + i + 1)
    })
    time.tick()
    i = i + 1
}
let d = test.liveAllocBytes() - base
console.log("churn delta=" + d.toString())
