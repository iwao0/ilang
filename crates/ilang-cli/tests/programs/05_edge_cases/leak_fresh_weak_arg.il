// Caller-releases-fresh extended to `MirTy::Weak`. Weak refs
// alias the strong object's pointer, but `__retain_weak` /
// `__release_weak` bump a side-table count that `__release_object`
// consults via `has_weaks` before freeing the body. The fresh-arg
// path was doubly broken:
//
// 1. `lower_block_hinted`'s tail-aliases-local retain ran BEFORE
//    the Object → Weak coerce, so `fn make_weak(): C.weak { let s
//    = new C(); s }` emitted `__retain_object` on the strong slot
//    that the scope-exit release was about to drop to zero. The
//    caller received a strong-+1 ptr typed as Weak; its
//    `__release_weak` table miss left the strong rc orphaned and
//    `__release_object` saw no weak entries, so it freed the body
//    on rc=0 — except no path ever decremented to 0, so the body
//    leaked.
//
// 2. `AssignField` for a `T.weak` field had the same shape: rhs
//    Object retain emitted `__retain_object` on a weak slot, so
//    `class H { w: C.weak; init(c: C) { this.w = c } }` skewed
//    the strong rc the same way (covered by `leak_weak_local_drop`,
//    just reinforced here).
//
// Fix coerces Object → Weak ahead of the retain in both call
// sites, then surfaces Weak in the paired needs_post_release /
// is_heap lists so the caller / field-assign side closes the
// loop.
//
// mir-jit only — `liveAllocCount` isn't wired into AOT.
// aot: skip
use std.test as test

class C { init() {} }

fn make_weak(): C.weak {
    let s = new C()
    s
}

class Holder {
    init() {}
    take(_w: C.weak) {}
    keep(w: C.weak): C.weak { w }
}

fn take_free(_w: C.weak) {}

fn round_free() {
    take_free(make_weak())
}

fn round_method() {
    let h = new Holder()
    h.take(make_weak())
    let _ = h
}

fn round_init() {
    let h = new Holder()
    let _ = h.keep(make_weak())
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
