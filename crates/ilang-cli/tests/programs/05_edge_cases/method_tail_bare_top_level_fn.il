// Class method whose tail is the bare name of a **top-level fn**
// (not a field). After the body_cx.rs fix, `tail_is_borrow` matches
// `ExprKind::Var(name)` when `this_class.is_some()` and the name
// resolves to neither env nor captures — the assumption is that such
// a name must be an implicit `this.field`. A top-level fn name with
// the same shape would also satisfy `lookup_binding=None` and
// `captures=None`, and an over-retain on a fn-pointer return would
// pin the closure trampoline (heap-allocated wrapper) per call and
// leak `~iters * trampoline_size`.
use std.test as test

fn helper(): i64 { 99 }

class Picker {
    init() {}
    pick(): fn(): i64 { helper }
}

let base = test.liveAllocBytes()
let i = 0
while i < 200 {
    let p = new Picker()
    let g = p.pick()
    test.expect(g(), 99)
    i = i + 1
}
let after = test.liveAllocBytes()
test.expectTrue(after - base < 4096)
