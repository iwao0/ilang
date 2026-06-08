// `let w: T.weak = ...; w = ...` was leaking a weak-table entry
// per reassignment. Pre-fix the `is_heap` match in
// `ExprKind::Assign` only listed `MirTy::Object`, so the prior
// weak slot's `__release_weak` was never called and the runtime
// weak-count table grew without bound.
//
// 09675349 had left Weak out of caller-releases-fresh ("the
// fresh-arg concept doesn't apply cleanly"). That reasoning
// doesn't carry over to slot reassignment: the weak slot owns
// exactly one weak-table entry's worth of rc, and overwriting
// the slot has to drop that share. `release_field_by_kind` /
// scope-exit / cascade all already drive Weak through this
// path; the assign path is the only one that lagged.
//
// The leak is in the weak-count table, which `liveAllocCount`
// tracks (each `__retain_weak` either reuses an entry or
// allocates a new `WeakEntry` via __mir_alloc).
//
// mir-jit only — `liveAllocCount` isn't wired into AOT.
// aot: skip
use std.test as test

class Counter {
    n: i64
    init() { this.n = 0 }
}

fn reassign_weak() {
    let c = new Counter()
    let w: Counter.weak = c
    let i: i64 = 0
    while i < 20 {
        w = c
        i = i + 1
    }
}

reassign_weak()

let base = test.liveAllocCount()
for _ in 0..50 { reassign_weak() }
test.expect(test.liveAllocCount() - base, 0)
