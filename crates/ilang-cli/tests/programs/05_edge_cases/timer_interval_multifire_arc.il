// `setInterval` beyond the smoke tests: an interval RE-ARMS and fires
// repeatedly until `clearInterval`, a `clearTimeout` before the due time
// cancels the callback entirely, double-clearing an id is a no-op, and an
// interval / timeout callback that captures a heap value keeps it alive
// across fires and releases it once done (no leak, no over-release).
// Existing setInterval coverage (live_alloc_probe_nonblocking.il) clears
// on the first fire, so multi-fire + capture ARC were unpinned.
//
// expect: ok
use std.time as time
use std.test as test

let deinits: i64[] = [0]
class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

// --- interval fires repeatedly, self-clears after 3, capturing heap ---
let captured = new Box(7)
let sum: i64[] = [0]
let count: i64[] = [0]
let id: i64[] = [0]
id[0] = time.setInterval(fn() {
    sum[0] = sum[0] + captured.n      // reads the captured Box every fire
    count[0] = count[0] + 1
    if count[0] >= 3 {
        time.clearInterval(id[0])
    }
}, 1)
let i = 0
while i < 6 {
    time.sleep(2)
    time.tick()
    i = i + 1
}
test.expect(count[0], 3)              // re-armed and fired exactly 3 times
test.expect(sum[0], 21)              // 7 * 3

// --- clearTimeout before the due time cancels the callback ---
let fired: i64[] = [0]
let tid = time.setTimeout(fn() { fired[0] = 1 }, 5)
time.clearTimeout(tid)
time.sleep(8)
time.tick()
test.expect(fired[0], 0)

// --- double-clear (already-cleared / already-fired id) is a no-op ---
time.clearInterval(id[0])
time.clearTimeout(tid)

// --- churn: many timeouts each capturing a fresh heap; all reclaimed ---
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let j = 0
while j < 50 {
    let b = new Box(j)
    let _ = time.setTimeout(fn() { sum[0] = sum[0] + b.n }, 1)
    j = j + 1
}
time.sleep(3)
time.tick()
test.expect(deinits[0] - bd, 50)     // every captured Box deinits once
test.expect(test.liveAllocBytes() - d0, 0)

console.log("ok")
