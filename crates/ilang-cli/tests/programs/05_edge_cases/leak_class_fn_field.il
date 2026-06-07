// `new C(fresh_fn)` storing the closure into a `fn(...)` field —
// `lower_new`'s post-call release only covered `MirTy::Object`
// fresh args, leaving fresh `Fn` (anonymous closure literal,
// top-level fn trampoline) leaking one cell per construction.
// Confirm the count returns to baseline after a loop.
//
// mir-jit only — `liveAllocCount` isn't wired into AOT.
// aot: skip
use std.test as test

class Box {
    cb: fn(i64): i64
    init(c: fn(i64): i64) { this.cb = c }
}

fn dbl(n: i64): i64 { n + 1 }

fn one_round() {
    // Anonymous closure literal.
    let _b1 = new Box(fn(n: i64): i64 { n * 2 })
    // Top-level fn trampoline.
    let _b2 = new Box(dbl)
    let _ = 0
}

one_round()
let baseline = test.liveAllocCount()
for _ in 0..50 {
    one_round()
}
let diff = test.liveAllocCount() - baseline
test.expect(diff, 0)
