// `setInterval(cb, ms)` keeps firing until `clearInterval(id)`.
// Run-to-completion: nothing fires while main's own code runs —
// the end-of-program drain services the timer heap, re-arming the
// interval each round until the callback cancels itself on the
// third tick (JS-style self-clearing counter). Confirms the
// cancellation actually stops the loop: a missed clear would hang
// the drain forever, and an extra firing would print twice.
//
// expect: ticked 3

use std.time as time

let counter: i64[] = [0]
let id_holder: i64[] = [0]

let id = time.setInterval(fn() {
    counter[0] = counter[0] + 1
    if counter[0] >= 3 {
        time.clearInterval(id_holder[0])
        console.log("ticked " + counter[0].toString())
    }
}, 1)
id_holder[0] = id
