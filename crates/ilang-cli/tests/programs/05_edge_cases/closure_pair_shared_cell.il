// Two closures built in the SAME scope, each capturing the same
// mutable outer-scope name, SHARE one cell (JS / Swift-default
// semantics): a write through one is visible to the other and to the
// outer scope. The name is captured-and-mutated, so the `let` binds a
// shared heap cell that every capturer references.
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
test.expect(read(), 3)         // read sees bump's writes (shared cell)

// Independent makers still get their own cell — sharing is within a
// single `makePair()` invocation, not across calls.
let q = makePair()
let read2 = q[0]
let bump2 = q[1]
bump2()
test.expect(read2(), 1)
test.expect(read(), 3)         // first pair unaffected
