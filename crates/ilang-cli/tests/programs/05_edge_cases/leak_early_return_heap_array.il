// A function with an early `return` for the empty case AND a
// fall-through tail expression — both paths produce a heap-typed
// array. The caller's binding holds the returned array; on its
// drop the array's data buffer + header must reclaim. Pre-fix a
// missing release on the early-return path (or the tail) would
// leak one array per iteration that hits that branch.
// `return_from_loop_body.il` covers functional return-unwind;
// this fixture pins down the heap-result reclaim across both
// return shapes.
//

use std.test as test
fn maybeLong(n: i64): i64[] {
    if n < 0 { return [] }
    let xs: i64[] = []
    let i: i64 = 0
    while i < n {
        xs.push(i)
        i = i + 1
    }
    xs
}

let baseline = test.liveAllocBytes()

let i: i64 = 0
while i < 200 {
    let _ = maybeLong(i % 5).length
    i = i + 1
}

let after = test.liveAllocBytes()
test.expectTrue(after - baseline < 65536)
