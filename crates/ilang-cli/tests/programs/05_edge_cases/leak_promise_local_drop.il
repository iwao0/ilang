// `Promise<T>` locals were never released at scope exit because
// `release_top_scope_objects`'s `needs_release` list omitted
// `MirTy::Promise`. Adding it alone breaks the async-fn wrapper
// contract: the wrapper's tail returns the wrapper-local pending
// promise (`Promise.$promise.pending()`), and without a paired retain
// in `tail_needs_retain` / `emit_callee_retain` the scope-exit release
// drops the caller's `+1` share. This fixture exercises both halves
// — heap-typed payloads (string / Object) round-tripping through
// `Promise.resolve(...)` locals.
use std.test as test

class N { v: i64; init(x: i64) { this.v = x } }

// 1) Promise<string> locals drop without leaking the resolved value.
let base = test.liveStringCount()
let n = 0
while n < 200 {
    let p = Promise.resolve("v" + n.toString())
    n = n + 1
}
test.expectTrue(test.liveStringCount() - base < 64)

// 2) Promise<Object> locals drop without leaking the wrapped object.
let base2 = test.liveAllocCount()
let m = 0
while m < 200 {
    let p = Promise.resolve(new N(m))
    m = m + 1
}
test.expectTrue(test.liveAllocCount() - base2 < 64)
