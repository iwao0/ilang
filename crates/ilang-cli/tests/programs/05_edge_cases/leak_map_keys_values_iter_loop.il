// `m.keys()` and `m.values()` snapshot the map into a fresh
// `K[]` / `V[]`. Each call allocates a new array; iterating it
// with `for-in` consumes and drops it. A regression that retains
// the snapshot beyond the loop's life would leak per call.
// `map_keys_values_iteration.il` covers the functional behaviour;
// this fixture pins down the no-leak loop.
//
// jit: skip

use test

let m = new Map<i64, i64>()
m[1] = 10
m[2] = 20
m[3] = 30

let baseline = test.liveAllocBytes()

let i: i64 = 0
while i < 200 {
    let total = 0
    for k in m.keys() { total += k }
    for v in m.values() { total += v }
    let _ = total
    i = i + 1
}

let after = test.liveAllocBytes()
test.expectTrue(after - baseline < 65536)
