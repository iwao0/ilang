// `i64[][]` literal in a loop — the outer array's drop has to
// cascade-release each inner array (which itself frees its data
// buffer). The kind tag for the outer array must be KIND_ARRAY
// so `host_release_array` recurses; pre-fix a KIND_NONE outer
// would silently leak each inner cell + buffer per iteration.
// `leak_nested_heap_cascade.il` exercises a similar shape via
// the Optional<Array> cascade; this fixture pins down the plain
// `T[][]` literal.
//

use test

let baseline = test.liveAllocBytes()

let i: i64 = 0
while i < 100 {
    let xs: i64[][] = [[1, 2], [3, 4], [5, 6]]
    let _ = xs[0][0]
    i = i + 1
}

let after = test.liveAllocBytes()
test.expectTrue(after - baseline < 65536)
