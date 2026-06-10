// Method body whose tail is an `if/else` expression where each arm
// is a bare-var implicit field reference. Before the in_fn_body_top
// gating, `lower_block_hinted` applied the borrow-tail retain to
// every block it lowered, including the arm blocks — so a method
// returning `if flag { a } else { b }` retained twice (once per arm,
// once at the fn-body join) and leaked one rc per call. The arm
// retains are unnecessary because the receiving join block at the
// fn-body level re-issues its own retain.
use std.test as test

class Box { n: i64; init(x: i64) { this.n = x } }

class Wrap {
    a: Box
    b: Box
    init(x: Box, y: Box) { this.a = x; this.b = y }
    pick(flag: bool): Box {
        if flag { a } else { b }
    }
}

let base = test.liveAllocBytes()
let i = 0
while i < 200 {
    let xa = new Box(i)
    let xb = new Box(i * 2)
    let w = new Wrap(xa, xb)
    test.expect(w.pick(true).n, i)
    test.expect(w.pick(false).n, i * 2)
    i = i + 1
}
let after = test.liveAllocBytes()
test.expectTrue(after - base < 4096)
