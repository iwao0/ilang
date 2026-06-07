// Returning a closure mid-body (`if cond { return closure }`)
// still keeps the mutable cell alive — the closure carries an
// ARC handle on the cell, so the early-return path doesn't
// reclaim it before the caller invokes the closure.
use std.test as test

fn makeEarly(go_pos: bool): fn(): i64 {
    let n = 0
    if go_pos {
        return fn(): i64 {
            n = n + 10
            n
        }
    }
    fn(): i64 {
        n = n - 1
        n
    }
}

let pos = makeEarly(true)
test.expect(pos(), 10)
test.expect(pos(), 20)
test.expect(pos(), 30)

let neg = makeEarly(false)
test.expect(neg(), -1)
test.expect(neg(), -2)

// Independent calls don't share cells.
let pos2 = makeEarly(true)
test.expect(pos2(), 10)
test.expect(pos(), 40)
