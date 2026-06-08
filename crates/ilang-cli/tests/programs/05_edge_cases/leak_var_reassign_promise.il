// `let p: Promise<...> = ...; p = ...` was leaking the prior
// promise on every reassignment. Pre-fix the `is_heap` match in
// `ExprKind::Assign` only listed `MirTy::Object`, so a Promise
// slot's old occupant was never released — `__release_promise`
// is what drives the resolved payload (string / Object) to drop,
// so every iteration left both the promise's `AtomicI64`-headed
// box and its inner payload pinned.
//
// 09675349 had deliberately left Promise out of the
// caller-releases-fresh contract ("separate ownership path"),
// but slot reassignment is a different contract — the rc-slot
// kind list in `MirTy::is_heap` already includes Promise, and
// the runtime's `release_field_by_kind` cascade does cascade
// through Promise. Reassignment must match.
//
// mir-jit only — `liveAllocCount` isn't wired into AOT.
// aot: skip
use std.test as test

class N {
    v: i64
    init(x: i64) { this.v = x }
}

fn reassign_promise_str() {
    let p: Promise<string> = Promise.resolve("init")
    let i: i64 = 0
    while i < 20 {
        p = Promise.resolve("v" + i.toString())
        i = i + 1
    }
}

fn reassign_promise_obj() {
    let p: Promise<N> = Promise.resolve(new N(-1))
    let i: i64 = 0
    while i < 20 {
        p = Promise.resolve(new N(i))
        i = i + 1
    }
}

reassign_promise_str()
reassign_promise_obj()

let base = test.liveAllocCount()
for _ in 0..50 { reassign_promise_obj() }
test.expect(test.liveAllocCount() - base, 0)

let base_str = test.liveStringCount()
for _ in 0..50 { reassign_promise_str() }
test.expectTrue(test.liveStringCount() - base_str < 64)
