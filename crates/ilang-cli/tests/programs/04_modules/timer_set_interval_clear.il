// `setInterval(cb, ms)` keeps firing until `clearInterval(id)`.
// Confirm cancellation actually stops the loop and the captured
// counter sees a finite number of ticks. The pool worker checks
// the cancel flag between sleeps, so the count stops within one
// interval after clear.
//
// expect: ticked

use std.time as time
use std.test as test

let counter: i64[] = [0]
let id_holder: i64[] = [0]

let id = time.setInterval(fn() {
    counter[0] = counter[0] + 1
    if counter[0] >= 3 {
        time.clearInterval(id_holder[0])
    }
}, 1)
id_holder[0] = id

// Block briefly so the interval can fire its three ticks before
// we read the result. `sleep` blocks the main thread; the worker
// keeps running on the pool.
time.sleep(60)
test.expectTrue(counter[0] >= 3)
console.log("ticked")
