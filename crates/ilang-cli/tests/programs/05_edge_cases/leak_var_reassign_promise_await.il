// Reassigning a `Promise<T>` slot inside an `async fn` where
// the value comes through `await`. The async lowering wraps the
// body in a pending-promise wrapper, so the rc accounting around
// `await` / slot reassign has its own subtle path on top of the
// caller-releases-fresh contract.
//
// Pre-fix the variable-reassignment cascade only released Object
// slots; a Promise slot reassigned with the result of another
// `Promise.resolve(...)` (the typical await source) leaked the
// prior promise plus its resolved payload. This fixture wraps
// the reassignment in an `async fn run()` so the leak path
// exercises the wrapper's tail-promise plumbing too.
//
// mir-jit only — liveAllocCount isn't wired into AOT.
// aot: skip
use std.test as test

class N {
    v: i64
    init(x: i64) { this.v = x }
}

fn make_promise(x: i64): Promise<N> {
    Promise.resolve(new N(x))
}

async fn reassign_in_async(): i64 {
    let p: Promise<N> = make_promise(0)
    let i: i64 = 0
    while i < 20 {
        p = make_promise(i)
        i = i + 1
    }
    let n: N = await p
    n.v
}

let _ = reassign_in_async().then(fn(_n: i64) {})

let base = test.liveAllocCount()
for _ in 0..50 {
    let _ = reassign_in_async().then(fn(_n: i64) {})
}
test.expectTrue(test.liveAllocCount() - base < 64)
