// Regression guard: the event-loop drain at program exit must run while
// the top-level globals are still alive.
//
// A pending Promise continuation can hold a heap object whose `deinit`
// touches a top-level global (here `deinits[0] = ...`). The drain that
// flushes pending continuations used to run only AFTER `__main`
// returned (in `run_main` for the JIT, and the C `main` wrapper for AOT)
// — i.e. AFTER `__main`'s epilogue had already freed the top-level
// `let`s. So when the held Box was finally released during that late
// drain, its `deinit` dereferenced the freed `deinits` array and the
// runtime aborted with "index out of bounds".
//
// Fixed by emitting a `promise_drain` at the end of `__main`'s body,
// before the top-level lets are released. This fixture leaves a pending
// promise un-ticked so the shutdown drain is exercised: pre-fix it
// crashed (exit 1); post-fix it exits cleanly.
//
// The leak dimension is checked separately with an explicit `time.tick()`
// drain so `deinit` counts are observable from top-level code (the
// shutdown drain runs after the last statement, so it can't be asserted
// inline).
//
// expect: ticked deinits=3
// expect: leaving a pending promise for shutdown drain

use std.test as test
use std.time as time

let deinits: i64[] = [0]

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

async fn makeBox(x: i64): Box {
    new Box(x)
}

// Awaits a heap-returning async fn, so the Box is held by the pending
// continuation across the await rather than freed synchronously.
async fn run(x: i64): i64 {
    let b: Box = await makeBox(x)
    b.n
}

// Part 1 — drain explicitly with time.tick() so the Box deinits are
// observable. 3 promises -> 3 Boxes, all freed by the ticks.
let i = 0
while i < 3 {
    let _ = run(1).then(fn(_n: i64) { })
    time.tick()
    i = i + 1
}
console.log("ticked deinits=" + deinits[0].toString())

// Part 2 — leave a pending promise un-ticked. Its Box is freed only by
// the shutdown drain; that drain must run before the `deinits` global is
// torn down, or `deinit` aborts on freed memory. Reaching the next line
// (clean exit) is the guard.
let _ = run(9).then(fn(_n: i64) { })
console.log("leaving a pending promise for shutdown drain")
