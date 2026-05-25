// `o.f = new_value` where `f` is heap-typed: the field-store path
// has to release the field's previous value BEFORE the new one
// lands. Pre-fix a missing release in the field-write lowering
// silently leaked the prior array on every iteration. The same
// shape applies to `o.s = "..."` for string fields and (covered
// separately) to closure-typed fields.
//

use std.test as test
class Holder {
    arr: i64[]
    init() { this.arr = [] }
}

let baseline = test.liveAllocBytes()

let h = new Holder()
let i: i64 = 0
while i < 200 {
    h.arr = [i, i + 1, i + 2]
    let _ = h.arr[0]
    i = i + 1
}

let after = test.liveAllocBytes()
// Only the FINAL array survives at end-of-loop (held by `h.arr`);
// every prior iteration's array must have dropped at the next
// store.
test.expectTrue(after - baseline < 32768)
