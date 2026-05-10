// `xs.map(...).filter(...)` produces TWO intermediate fresh arrays
// per iteration — the map result and the filter result. Both must
// drop at scope exit. A regression that retains the map result
// (because filter's receiver is fresh-not-aliased and gets an extra
// retain instead of consume) would leak ~24 + N×stride bytes per
// loop body. Combines with `leak_array_map_heap_result.il` which
// covers the heap-element cascade for a single map call.
//
// jit: skip

use test

let baseline_b = test.liveAllocBytes()
let baseline_s = test.liveStringCount()

let i: i64 = 0
while i < 500 {
    let xs = [i, i * 2, i * 3]
    let mapped = xs.map(fn(x: i64): i64 { x + 1 })
    let filtered = mapped.filter(fn(x: i64): bool { x > 0 })
    let _ = filtered
    i = i + 1
}

let after_b = test.liveAllocBytes()
let after_s = test.liveStringCount()
test.expectTrue(after_b - baseline_b < 32768)
test.expectTrue(after_s - baseline_s < 50)
