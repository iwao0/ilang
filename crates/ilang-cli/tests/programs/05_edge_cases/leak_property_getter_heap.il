// A computed property whose getter returns a heap-typed value
// (here: an array reference into a field). Per call, the getter
// produces a +1 on the array; the caller's `.length` consumes it
// and the temporary drops. A regression that retains the array
// without a balancing release would leak per call.
//
// `property_cross_getter.il` covers the cross-getter behaviour;
// this fixture pins down the heap-result reclaim across many
// calls.
//

use std.test

class Vec {
    coords: i64[]
    init() { this.coords = [10, 20, 30] }
    get xs(): i64[] { this.coords }
}

let baseline = test.liveAllocBytes()

let v = new Vec()
let i: i64 = 0
while i < 500 {
    let _ = v.xs.length
    i = i + 1
}

let after = test.liveAllocBytes()
test.expectTrue(after - baseline < 32768)
