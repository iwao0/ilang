// A closure capturing a heap-typed value (Array here) must
// retain the captured array on construction and release it on
// closure drop. Combined with the closure-cell free
// (`leak_closure_cell_loop.il`), the loop should reclaim both
// the cell and the captured array on every iteration.
//
// `closure_capture_runs_deinit.il` covers deinit-trace for a
// captured class instance; this fixture pins down the actual
// memory reclaim using `liveAllocBytes`.
//
// jit: skip

use test

let baseline = test.liveAllocBytes()

let i: i64 = 0
while i < 1000 {
    // Each iteration builds a fresh Array<i64> (= 48-byte header
    // + cap*8 data) and a closure cell (= 16 + 1*8 = 24 bytes)
    // capturing it. Both must be reclaimed at scope exit.
    let xs: i64[] = [i, i + 1, i + 2]
    let f = fn(): i64 { xs[0] + xs[1] + xs[2] }
    let _ = f()
    i = i + 1
}

let after = test.liveAllocBytes()
let grew = after - baseline

// Pre-fix: leaks both array (88 B) + closure cell (24 B) per
// iter ≈ 112 KB total. With both freed, growth bounded.
test.expectTrue(grew < 1024)
