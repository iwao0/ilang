// Closures own a private cell per captured variable. The cell is
// snapshotted from the outer binding at creation time, then becomes
// the closure's own state — mutations inside the body persist across
// repeated invocations of the same closure, and different closures
// (even built in the same scope) get independent cells.
use std.test as test
fn makeCounter(): fn(): i64 {
    let n = 0
    fn(): i64 { n = n + 1; n }
}

let c1 = makeCounter()
let c2 = makeCounter()

// Mutations through c1 accumulate.
test.expect(c1(), 1)
test.expect(c1(), 2)
test.expect(c1(), 3)

// c2 has its own cell — independent of c1.
test.expect(c2(), 1)
test.expect(c2(), 2)

// Interleaving: each closure tracks its own count.
test.expect(c1(), 4)
test.expect(c2(), 3)

// A closure that captures by SUMMING into the captured cell.
fn makeAccum(): fn(i64): i64 {
    let total = 0
    fn(x: i64): i64 { total = total + x; total }
}
let acc = makeAccum()
test.expect(acc(10), 10)
test.expect(acc(5), 15)
test.expect(acc(-3), 12)

// Outer rebind (`let n = 999` shadowing) must NOT leak into a
// previously-built closure — its cell was snapshotted at creation.
let n = 10
let read = fn(): i64 { n }
test.expect(read(), 10)
let n = 999
test.expect(read(), 10)         // unchanged

// Outer plain assignment after creation also doesn't reach in
// (the cell is the closure's private storage, not a shared slot).
let m = 7
let read_m = fn(): i64 { m }
test.expect(read_m(), 7)
m = 100
test.expect(read_m(), 7)
