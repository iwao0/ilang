// `if let some(x) = opt { ... }` extracts the inner value from
// the Optional cell. The cell's +1 was the let-binding's, and
// when the binding falls out of scope at loop-end the cell drops.
// `leak_optional_cell_loop.il` covers the bare `let opt = some(x)`
// shape; this fixture adds the if-let extraction path which has a
// distinct retain/release shape (the inner is read out, the shell
// is dropped).
//
// jit: skip

use test

let baseline = test.liveAllocBytes()

let i: i64 = 0
while i < 500 {
    let opt: i64? = some(i)
    let v = if let some(x) = opt { x } else { 0 }
    let _ = v
    i = i + 1
}

let after = test.liveAllocBytes()
test.expectTrue(after - baseline < 32768)
