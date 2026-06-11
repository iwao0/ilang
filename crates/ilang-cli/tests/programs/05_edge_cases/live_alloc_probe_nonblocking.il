// `test.liveAllocBytes()` pumps the event loop (queued
// continuations + already-due timers) and returns — it must NOT
// sleep until a pending timer's due time. With an armed interval a
// blocking drain would never return at all, so this fixture pins
// "probe returns while an interval is armed". The interval clears
// itself on its first firing so the end-of-program drain (which DOES
// wait for timers, by design) still terminates.
//
// expect: probe returned with interval armed

use std.time as time
use std.test as test

let id_h: i64[] = [0]
id_h[0] = time.setInterval(fn() {
    time.clearInterval(id_h[0])
}, 30)

let _ = test.liveAllocBytes()
let _ = test.liveAllocCount()
console.log("probe returned with interval armed")
