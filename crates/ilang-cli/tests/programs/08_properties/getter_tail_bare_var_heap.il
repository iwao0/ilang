// Property getter body returning the bare field name (`inner`) of a
// heap-typed field. Same desugar shape as the `Box<T>.get(): T { x }`
// regression that ~16% race-aborted under parallel-spawn before
// body_cx.rs's `tail_is_borrow` learned to match `Var` in a class
// method. This fixture asserts both the value AND that the heap is
// drained at scope end (no over-retain leak from the fix).
use std.test as test

class Box {
    v: i64
    init(x: i64) { this.v = x }
}

class Wrap {
    inner: Box
    init(b: Box) { this.inner = b }
    get value(): Box { inner }
}

let base = test.liveAllocBytes()
let i = 0
while i < 200 {
    let b = new Box(i)
    let w = new Wrap(b)
    test.expect(w.value.v, i)
    i = i + 1
}
let after = test.liveAllocBytes()
// 200 iters with the bug would either crash from a use-after-free
// (rc short) or pin a few KB of leaked Box+Wrap pairs; demand the
// scope-exit cascade drained to a small bounded growth.
test.expectTrue(after - base < 2048)
