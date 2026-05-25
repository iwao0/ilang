// `T[]?` — an Optional whose inner is itself heap-typed. The
// Optional cell carries +1 on the array; if the cell is `none`,
// no array allocated; if `some(arr)`, both must drop on scope
// exit. `leak_optional_cell_loop.il` covers `i64?` (primitive
// payload — no inner heap to cascade); `leak_nested_heap_cascade
// .il` exercises Optional<Array> through a different shape; this
// fixture is the simple "fn returning T[]?, if-let extract" loop.
//

use std.test as test
fn pick(n: i64): i64[]? {
    if n % 2 == 0 {
        return some([n, n + 1])
    }
    none
}

let baseline = test.liveAllocBytes()

let i: i64 = 0
while i < 200 {
    let opt = pick(i)
    let v = if let some(arr) = opt { arr.length } else { 0 }
    let _ = v
    i = i + 1
}

let after = test.liveAllocBytes()
test.expectTrue(after - baseline < 32768)
