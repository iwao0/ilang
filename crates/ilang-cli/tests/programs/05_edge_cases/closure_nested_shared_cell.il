// Nested closures: when an outer wrapper has a mutable capture and
// the body builds an inner closure that captures the SAME name, the
// inner shares the outer's cell — writes through the inner are
// visible to the outer body too. Pre-fix the JIT's hoist pass scanned
// the body for mutability AFTER recursive hoisting, by which time the
// inner FnExpr had been rewritten to an opaque Closure node and its
// `n = ...` was no longer detectable: the outer's capture stayed
// inline, the inner snapshotted a fresh value, and writes vanished.
use std.test as test
fn make(): fn(): i64 {
    let n = 0
    fn(): i64 {
        let g = fn() { n = n + 1 }
        g()
        g()
        n
    }
}
let h = make()
test.expect(h(), 2)         // first call: g bumped twice
test.expect(h(), 4)         // cell persists across calls
test.expect(h(), 6)

// Two independent makers — each h's cell is its own.
let h2 = make()
test.expect(h2(), 2)
test.expect(h(), 8)         // h's cell still distinct, +2 again
