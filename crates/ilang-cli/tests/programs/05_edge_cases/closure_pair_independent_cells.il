// Two closures built in the SAME scope, each capturing the same
// outer-scope name. They get independent cells (B1 semantics) — a
// write through one is invisible to the other. Sharing only happens
// when one closure is built INSIDE another that already has a
// mutable capture of the same name (covered separately by
// closure_nested_shared_cell.il).
use std.test as test
fn makePair(): (fn(): i64, fn()) {
    let n = 0
    let read = fn(): i64 { n }
    let bump = fn() { n = n + 1 }
    (read, bump)
}
let p = makePair()
let read = p[0]
let bump = p[1]

test.expect(read(), 0)
bump()
bump()
bump()
test.expect(read(), 0)         // read's cell is independent

// Reading via the bump-side closure is NOT something we exercise
// (the lambda doesn't return n) — but multiple reads of `read` keep
// returning the snapshotted 0.
test.expect(read(), 0)
test.expect(read(), 0)
