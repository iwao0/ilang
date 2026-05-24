// `xs[i] += v` / `xs[i] *= v` etc. — compound assignment on an
// array index l-value. The lowering reads the current value, runs
// the binary op, then stores back. For primitive elements there's
// no retain/release cascade per step, but a regression that
// re-allocates the array on each compound assign (or fails to
// free a temp) would show up as steady growth. `compound_assign
// _index_and_field.il` covers the functional behaviour; this
// fixture pins down the no-leak loop.
//

use std.test

let baseline = test.liveAllocBytes()

let xs: i64[] = [1, 2, 3]
let i: i64 = 0
while i < 1000 {
    xs[0] += 1
    xs[1] *= 2
    xs[1] /= 2
    xs[2] -= 1
    xs[2] += 1
    let _ = xs[0]
    i = i + 1
}

let after = test.liveAllocBytes()
test.expectTrue(after - baseline < 32768)
