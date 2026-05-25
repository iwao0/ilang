// An outer closure factory that mutates its own captured `n` on each
// call AND, before returning, snapshots the current `n` into a fresh
// local. The inner closure captures THAT local — so each inner
// remembers its own per-call value, while the outer keeps counting
// up across calls.
use std.test as test
fn outer(): fn(): fn(): i64 {
    let n = 0
    fn(): fn(): i64 {
        n = n + 1
        let snapshot = n
        fn(): i64 { snapshot }
    }
}

let f = outer()
let g1 = f()                   // n becomes 1, snapshot = 1
let g2 = f()                   // n becomes 2, snapshot = 2
let g3 = f()                   // n becomes 3, snapshot = 3

test.expect(g1(), 1)           // each g remembers its own snapshot
test.expect(g2(), 2)
test.expect(g3(), 3)

// Re-invoking g1 returns the same snapshot — it's not re-derived.
test.expect(g1(), 1)
test.expect(g3(), 3)

// A second outer-factory invocation is independent.
let f2 = outer()
let h = f2()
test.expect(h(), 1)            // f2's own n started at 0 → 1
test.expect(g1(), 1)           // f's chain unaffected
