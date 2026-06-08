// `let f: fn(): T = ...; f = ...` for closure-typed locals.
// `MirTy::Fn` is on `is_heap` because Fn values are box-allocated
// closures (env capture cell + function ptr). Pre-fix the Assign
// cascade's Object-only `is_heap` match leaked the prior closure
// on every reassignment, holding its captured environment too.
//
// Probe: the inner closure captures a Tag, so liveAllocCount sees
// the leaked Tag if the prior closure isn't released.
//
// mir-jit only — liveAllocCount isn't wired into AOT.
// aot: skip
use std.test as test

@derive(Eq, Hash)
class Tag {
    pub n: i64
    init(n: i64) { this.n = n }
}

fn make_closure(t: Tag): fn(): i64 {
    fn(): i64 { t.n }
}

fn round() {
    let f: fn(): i64 = make_closure(new Tag(0))
    let i: i64 = 0
    while i < 20 {
        f = make_closure(new Tag(i))
        i = i + 1
    }
    let _ = f()
}

round()
let base = test.liveAllocCount()
for _ in 0..50 { round() }
test.expect(test.liveAllocCount() - base, 0)
