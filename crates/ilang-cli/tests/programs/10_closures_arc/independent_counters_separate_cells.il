// Two separate `makeCounter()` calls each get their own shared
// cell — incrementing one does not bleed into the other. Per
// syntax.md §6 "Independent `let` bindings (e.g. two separate
// `makeCounter()` calls) still get their own cells".
use std.test as test

fn makeCounter(): fn(): i64 {
    let n = 0
    fn(): i64 {
        n = n + 1
        n
    }
}

let c1 = makeCounter()
let c2 = makeCounter()

test.expect(c1(), 1)
test.expect(c1(), 2)
test.expect(c2(), 1)   // c2 has its own cell, still at 0 before this call
test.expect(c1(), 3)
test.expect(c2(), 2)
