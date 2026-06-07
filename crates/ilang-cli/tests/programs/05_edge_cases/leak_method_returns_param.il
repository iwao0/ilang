// Method / fn that returns its non-this heap param as the tail.
// Pre-fix this pattern leaked one cell per call — two related
// causes: (1) lower_block_hinted's tail-alias retain fired for ANY
// `Var` tail, including params (which scope-exit doesn't touch),
// leaving an unbalanced +1; (2) a heap-typed tail in a Unit-
// returning caller (`a.same(b)` as the body's last expression with
// no semicolon) had no Release emitted at finalise_return time,
// so the method's callee-retain rested on the value forever.
//
// mir-jit only — `liveAllocCount` isn't wired into AOT.
// aot: skip
use std.test as test

class Box {
    n: i64
    init(n: i64) { this.n = n }
    // Method returning its non-this heap param.
    same(o: Box): Box { o }
}

fn passthrough(b: Box): Box { b }

fn discard_round() {
    let a = new Box(1)
    let b = new Box(2)
    // Heap-typed tail discarded by Unit-returning caller.
    a.same(b)
}

fn capture_round() {
    let a = new Box(1)
    let b = new Box(2)
    let _c = a.same(b)
    let _ = 0
}

fn freefn_round() {
    let b = new Box(7)
    let _c = passthrough(b)
    let _ = 0
}

discard_round()
capture_round()
freefn_round()

let baseline = test.liveAllocCount()
for _ in 0..50 {
    discard_round()
    capture_round()
    freefn_round()
}
let diff = test.liveAllocCount() - baseline
test.expect(diff, 0)
