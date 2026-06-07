// `setTimeout(cb, ms)` schedules `cb` after `ms` milliseconds.
// The runtime drains pending timers at program end (mirrors
// Node's "wait for the event loop"), so a 0ms timer reliably
// fires before exit. Confirms the timer pool path doesn't drop
// the callback closure prematurely.
//
// expect: tick
use std.time as time

let _id = time.setTimeout(fn() {
    console.log("tick")
}, 0)

let _ = 0
