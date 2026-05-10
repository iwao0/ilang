// A class with no fields — just methods. The `new` allocation is
// a 16-byte header-only object. Currently the class-cell free is
// guarded (see `leak_object_alloc_loop.il`), so the bytes baseline
// is intentionally loose; once the cell-free lands, this fixture
// should shrink to a tight bound. Pre-fix even no-field classes
// allocated a fields slot for KIND tags — the recent layout work
// dropped that, so a regression that re-adds it would show up
// here.
//
// jit: skip

use test

class Adder {
    add(a: i64, b: i64): i64 { a + b }
    sub(a: i64, b: i64): i64 { a - b }
    mul(a: i64, b: i64): i64 { a * b }
}

let baseline = test.liveAllocBytes()

let i: i64 = 0
while i < 300 {
    let a = new Adder()
    let _ = a.add(i, i * 2)
    let _ = a.sub(i * 3, i)
    let _ = a.mul(i, 7)
    i = i + 1
}

let after = test.liveAllocBytes()
test.expectTrue(after - baseline < 32768)
