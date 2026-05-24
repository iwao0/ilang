// A function constructs a heap-typed local AND returns a closure
// that captures it. The closure carries a +1 on the array; when
// the closure value drops, the captured array's last reference
// goes too, releasing both the closure cell and the array. This
// is the classic "escape via closure" shape — without proper
// retain-on-construct + release-on-drop the array would either
// double-free or leak.
//

use std.test

fn makeArrFn(): fn(): i64 {
    let xs: i64[] = [1, 2, 3, 4, 5]
    fn(): i64 { xs.length }
}

let baseline = test.liveAllocBytes()

let i: i64 = 0
while i < 200 {
    let f = makeArrFn()
    let _ = f()
    i = i + 1
}

let after = test.liveAllocBytes()
test.expectTrue(after - baseline < 65536)
