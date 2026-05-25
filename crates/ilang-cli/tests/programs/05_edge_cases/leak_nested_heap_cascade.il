// `i64[][]` (array of arrays) — the outer array's drop cascades
// release on every inner array, so nothing leaks.
//
// `host_release_array` reads the kind_tag header word and
// dispatches to the matching release fn (KIND_ARRAY →
// host_release_array recursively, KIND_OPTIONAL →
// host_release_optional, etc.). NewArray / NewArrayEmpty set
// the tag from the element MirTy via `kind_tag_of`.
//

use std.test as test
let baseline = test.liveAllocBytes()

let i: i64 = 0
while i < 1000 {
    // Outer Array<i64[]> with two inner arrays per iter.
    let xs: i64[][] = [[i, i + 1], [i + 2, i + 3, i + 4]]
    let _ = xs
    i = i + 1
}

let after = test.liveAllocBytes()
let grew = after - baseline

// With the cascade in place, growth is bounded by allocator
// overhead. Pre-fix would leak ~160 KB.
test.expectTrue(grew < 1024)
