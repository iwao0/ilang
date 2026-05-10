// `weak.get()` returns `T?` — when the target is alive it bumps the
// strong rc and produces `some(t)`. The caller's `if let some(s) = ...`
// extracts `s`, runs the body, then drops `s` (releasing the strong
// rc) AND drops the Optional shell. A regression that fails to
// release either the upgraded strong or the Optional cell would leak
// per call. `weak_basic.il` covers the functional behaviour; this
// fixture pins down the alloc-tracker reclaim across many upgrades.
//
// jit: skip

use test

class Counter {
    n: i64
    init() { this.n = 0 }
}

let baseline = test.liveAllocBytes()

let c = new Counter()
let i: i64 = 0
while i < 200 {
    let w: Counter.weak = c
    let upgraded = w.get()
    let _ = if let some(s) = upgraded { s.n } else { 0 }
    i = i + 1
}

let after = test.liveAllocBytes()
test.expectTrue(after - baseline < 32768)
