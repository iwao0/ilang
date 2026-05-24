// Tuple cascade dispatches per-element kind, not just Object.
// `(Array<i64>, Optional<i64>)` etc. must reclaim each element's
// heap allocation when the tuple drops.
//
// Pre-fix `host_release_tuple` cascaded only the Object bit-mask
// elements. Tuple-of-Array / Tuple-of-Optional / Tuple-of-Closure
// leaked their inner cells. Fix packs a 4-bit KIND_* tag per
// element into the tuple header (bits 16-63 of the packed word).
//

use std.test

let baseline = test.liveAllocBytes()

let i: i64 = 0
while i < 1000 {
    let t: (i64[], i64?) = ([i, i + 1], some(i))
    let _ = t
    i = i + 1
}

let after = test.liveAllocBytes()
let grew = after - baseline

// Pre-fix per iter: 32B tuple cell + ~88B array + 24B optional ≈
// 144B leaked. With cascade, growth is bounded by allocator
// overhead.
test.expectTrue(grew < 1024)
