// Method body whose tail is a `match` whose arms are bare-var
// implicit field references. Same shape as the if-arm regression
// (05_edge_cases/method_tail_bare_var_if_arm.il), but routed
// through match instead of if-else. Verifies that `in_fn_body_top`
// clears for match-arm sub-blocks too, so the borrow-tail retain
// only fires at the fn-body block.
use std.test as test

class Box { n: i64; init(x: i64) { this.n = x } }

enum Side { left, right }

class Wrap {
    a: Box
    b: Box
    init(x: Box, y: Box) { this.a = x; this.b = y }
    pick(s: Side): Box {
        match s {
            left { a }
            right { b }
        }
    }
}

let base = test.liveAllocBytes()
let i = 0
while i < 200 {
    let xa = new Box(i + 1)
    let xb = new Box(i * 2 + 1)
    let w = new Wrap(xa, xb)
    let r1 = w.pick(Side.left)
    let r2 = w.pick(Side.right)
    test.expect(r1.n, i + 1)
    test.expect(r2.n, i * 2 + 1)
    i = i + 1
}
let after = test.liveAllocBytes()
// 64-byte intern of the enum's two unit variants (`left` / `right`)
// is a one-time process-wide overhead, not iter-proportional.
test.expectTrue(after - base < 256)
