// `Map<i64, T[]>` with array values — index-write replacement
// must release the previous array. Per-iteration insertions of
// fresh arrays into a fresh Map must drop both the map cell and
// every array it held when the map binding falls out of scope.
//
// `leak_map_overwrite_loop.il` covers `Map<string, i64>` (no
// heap value cascade); this fixture adds the heap-value cascade
// path, where `host_release_map` has to walk values per kind
// tag.
//

use std.test

let baseline = test.liveAllocBytes()

let i: i64 = 0
while i < 100 {
    let m = new Map<i64, i64[]>()
    m[1] = [i, i + 1]
    m[2] = [i * 2, i * 3, i * 4]
    m[1] = [99, 100, 101, 102]   // replace — old array drops
    let _ = m[1].length
    i = i + 1
}

let after = test.liveAllocBytes()
test.expectTrue(after - baseline < 65536)
