// Event-loop ordering guarantees:
// - same-due timers fire in registration order (the heap's seq
//   tie-break)
// - queued continuations run before due timers at a drain point
//   (JS microtask-before-macrotask)
// - a 0ms timer scheduled from a timer callback is serviced within
//   the same pump (tick loops until nothing is runnable)
// - time.tick() may be called from inside a timer callback
//   (re-entrant pump) without deadlock or double-fire
//
// expect: same-due: abc
// expect: micro-macro: then,timer0
// expect: chain depth: 5
// expect: reentrant: 11

use std.time as time
use std.test as test

let order: string[] = []
let _a = time.setTimeout(fn() { order.push("a") }, 1)
let _b = time.setTimeout(fn() { order.push("b") }, 1)
let _c = time.setTimeout(fn() { order.push("c") }, 1)
time.sleep(3)
time.tick()
test.expect(order.length, 3)
console.log("same-due: " + order[0] + order[1] + order[2])

let seq: string[] = []
let _t0 = time.setTimeout(fn() { seq.push("timer0") }, 0)
let _p = Promise.resolve(1).then(fn(v: i64) { seq.push("then") })
time.tick()
console.log("micro-macro: " + seq[0] + "," + seq[1])

let depth: i64[] = [0]
fn chain() {
    depth[0] = depth[0] + 1
    if depth[0] < 5 {
        let _ = time.setTimeout(fn() { chain() }, 0)
    }
}
let _s = time.setTimeout(fn() { chain() }, 0)
time.tick()
console.log("chain depth: " + depth[0].toString())

let reent: i64[] = [0]
let _r = time.setTimeout(fn() {
    reent[0] = reent[0] + 1
    time.tick()
    reent[0] = reent[0] + 10
}, 0)
time.tick()
console.log("reentrant: " + reent[0].toString())
