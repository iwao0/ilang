// Per-iteration closure construction allocates a fresh
// `[fn_addr | rc | captures...]` cell. The cell must be freed
// when its rc reaches 0 — without it, every `let f = fn(): T { ... }`
// in a loop leaks the cell.
//
// Pre-fix `host_release_closure` cascaded heap-typed captures'
// release but never called `host_mir_free` on the cell itself
// (closure_capture_table didn't carry the size).
//

use std.test as test
let baseline = test.liveAllocBytes()

let i: i64 = 0
while i < 1000 {
    // Closure with one i64 capture: cell = [fn_addr|rc|i] = 24B.
    let f = fn(): i64 { i }
    // Invoke once so the optimizer can't eliminate the construction.
    let _ = f()
    i = i + 1
}

let after = test.liveAllocBytes()
let grew = after - baseline

// 1000 closure cells × 24 bytes = ~24 KB pre-fix. With cell free
// in place, growth is bounded by allocator overhead.
test.expectTrue(grew < 256)
