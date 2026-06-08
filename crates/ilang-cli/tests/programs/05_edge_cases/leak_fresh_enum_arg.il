// Caller-releases-fresh extended to `MirTy::Enum` args. Pre-fix
// the `needs_post_release` set in `call_fn.rs` /
// `calls/object.rs` / `calls/mod.rs` omitted Enum (matching the
// AssignField retain set, which also omitted it), so
// `take(Container.has(...))` and `obj.take(Container.has(...))`
// each leaked one enum cell per call when the callee didn't
// store the value. Class-init args were already balanced because
// `init` stashes them into a field whose subsequent
// release-cascade undoes the fresh +1.
//
// mir-jit only — `liveAllocCount` isn't wired into AOT.
// aot: skip
use std.test as test

enum Container {
    empty
    has: (i64)
}

class Holder {
    init() {}
    take(_c: Container) {}
    swap(c: Container): Container { c }
}

fn take(_c: Container) {}

fn round_free() {
    take(Container.has(7))
}

fn round_method() {
    let h = new Holder()
    h.take(Container.has(7))
    let _ = h
}

fn round_init() {
    // Init path was clean even before the fix — pinned to catch
    // regressions if AssignField stops retaining Enum without
    // also stripping `needs_post_release`.
    let h = new Holder()
    let _ = h.swap(Container.has(7))
    let _ = h
}

round_free()
round_method()
round_init()

let baseline = test.liveAllocCount()
for _ in 0..50 {
    round_free()
    round_method()
    round_init()
}
test.expect(test.liveAllocCount() - baseline, 0)
