// `clearTimeout(id)` cancels a pending timer before it fires —
// the callback never runs and its captured closure is released
// when the timer entry is dropped. A short non-cancelled timer
// fires to confirm the runtime didn't accidentally cancel the
// wrong slot.
//
// expect: kept

use std.time as time

let to_cancel = time.setTimeout(fn() {
    console.log("cancelled")
}, 200)
time.clearTimeout(to_cancel)

let _kept = time.setTimeout(fn() {
    console.log("kept")
}, 0)

let _ = 0
