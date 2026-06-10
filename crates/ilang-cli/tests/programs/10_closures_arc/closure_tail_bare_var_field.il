// Closure whose tail is `inner` — a bare-var implicit field
// reference of the enclosing method's `this`. Two pre-existing pieces
// of machinery had to compose: collect_free_vars_expr now adds `this`
// as a candidate free var alongside every bare-var (so lower_fn_expr
// picks `this` up from the enclosing method's env and stitches a
// closure capture for it), and lower_var_expr's implicit-this.field
// path now falls back to `LoadCapture` when `this` isn't in env but
// is captured. Without those two, `grab` panicked during lower with
// `Option::unwrap()` on None at expr.rs:1135.
use std.test as test

class Box {
    n: i64
    init(x: i64) { this.n = x }
}

class Holder {
    inner: Box
    init(b: Box) { this.inner = b }
    grab(): fn(): Box { fn(): Box { inner } }
}

let base = test.liveAllocBytes()
let i = 0
while i < 200 {
    let b = new Box(i + 7)
    let h = new Holder(b)
    let g = h.grab()
    test.expect(g().n, i + 7)
    i = i + 1
}
let after = test.liveAllocBytes()
test.expectTrue(after - base < 4096)
