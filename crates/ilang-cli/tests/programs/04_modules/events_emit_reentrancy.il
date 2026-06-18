// std.events: a listener that mutates the listener set DURING `emit`
// must not corrupt the in-flight walk. `emit` cached `n = arr.length`
// and indexed the LIVE array, so a listener calling `off` (which
// shrinks the array) made the next `arr[i]` index out of bounds —
// `panic: index out of bounds`. Fixed by iterating a snapshot taken
// before firing (Node.js semantics: the set fired is the one present
// at emit start; the removal takes effect for the NEXT emit).
//
// expect: log=1001
// expect: count=1
// expect: sigLog=1001
// expect: sigCount=1

use std.events as ev

// EventEmitter: f1 removes f2 mid-emit.
let bus = new ev.EventEmitter<i64>()
let log = [0]
let f2 = fn(x: i64) { log[0] = log[0] + 1000 }
let f1 = fn(x: i64) {
    log[0] = log[0] + 1
    let _ = bus.off("e", f2)
}
bus.on("e", f1)
bus.on("e", f2)
bus.emit("e", 0)
console.log("log=" + log[0].toString())
console.log("count=" + bus.listenerCount("e").toString())

// Signal: g1 removes g2 mid-emit.
let sig = new ev.Signal<i64>()
let slog = [0]
let g2 = fn(x: i64) { slog[0] = slog[0] + 1000 }
let g1 = fn(x: i64) {
    slog[0] = slog[0] + 1
    let _ = sig.remove(g2)
}
sig.add(g1)
sig.add(g2)
sig.emit(0)
console.log("sigLog=" + slog[0].toString())
console.log("sigCount=" + sig.count.toString())
