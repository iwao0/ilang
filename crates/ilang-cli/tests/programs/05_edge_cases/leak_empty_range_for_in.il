// `for x in 5..5 {}` and `for x in 7..=6 {}` are zero-iteration
// loops — the body never runs. The for-in lowering still has to
// set up the iter state (start/end checked once) but must not
// allocate anything. A regression that always allocates an iter
// scratch object regardless of iteration count would leak ~24
// bytes per empty-range hit.
//

use std.test as test
let baseline = test.liveAllocBytes()

let i: i64 = 0
while i < 1000 {
    let total = 0
    for x in 5..5 { total += x }       // half-open empty
    for x in 7..=6 { total += x }      // inclusive empty
    for x in 0..0 { total += x }       // both ends zero
    let _ = total
    i = i + 1
}

let after = test.liveAllocBytes()
test.expectTrue(after - baseline < 32768)
