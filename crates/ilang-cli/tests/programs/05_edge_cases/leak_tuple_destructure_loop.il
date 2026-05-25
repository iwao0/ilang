// `let (a, b) = pair` destructure pattern: the tuple cell is
// allocated by the literal (`(i, i*2)`), bound to `pair`, then
// destructured. After the loop body, both `pair` and the (a, b)
// locals fall out of scope. The tuple cell must be reclaimed each
// iteration. Pre-fix the destructure-bind path didn't release the
// source tuple after destructuring (only the bindings were
// scoped), leaving the cell stranded. `leak_tuple_cell_loop.il`
// covers the simple `let t = (...)` shape; this fixture pins down
// the destructure-bind release.
//

use std.test as test
let baseline = test.liveAllocBytes()

let i: i64 = 0
while i < 500 {
    let pair = (i, i * 2)
    let (a, b) = pair
    let _ = a + b
    i = i + 1
}

let after = test.liveAllocBytes()
test.expectTrue(after - baseline < 32768)
