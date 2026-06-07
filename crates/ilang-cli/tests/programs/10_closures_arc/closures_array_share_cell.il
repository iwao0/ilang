// Two closures stored in the same array share a single mutable
// capture — `push`-ing each closure separately doesn't snapshot
// the cell, so the reader closure sees the writer closure's
// mutations through the same shared backing storage. Per
// syntax.md §6 "shared cell" semantics.
use std.test as test

fn make(): (fn(): i64)[] {
    let n = 0
    let xs: (fn(): i64)[] = []
    // Writer: mutates `n` and returns the new value.
    xs.push(fn(): i64 { n = n + 1; n })
    // Reader: returns `n` as-is.
    xs.push(fn(): i64 { n })
    xs
}

let cs = make()
test.expect(cs[0](), 1)     // writer bumps n to 1
test.expect(cs[1](), 1)     // reader sees the same n
test.expect(cs[0](), 2)
test.expect(cs[1](), 2)
test.expect(cs[0](), 3)
test.expect(cs[0](), 4)
test.expect(cs[1](), 4)

// A fresh batch starts at 0 again — each `make()` allocates a
// new shared cell.
let cs2 = make()
test.expect(cs2[1](), 0)
test.expect(cs2[0](), 1)
test.expect(cs[1](), 4)     // original batch is untouched
