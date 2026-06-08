// Caller-releases-fresh extended to `MirTy::Set`. Pre-fix the
// `needs_post_release` set in `call_fn.rs` /
// `calls/object.rs` / `calls/mod.rs` omitted Set (matching the
// AssignField retain set, which also omitted it), so passing a
// fresh `Set<…>` to a callee that didn't store it leaked the
// whole set — including every element it held — once per call.
//
// `Set` itself is `Box`-allocated (not `__mir_alloc`-tracked),
// so the probe goes through a class element whose retain count
// rides on the set's lifetime: `Set<Marker>` with one inserted
// `Marker` per round. If the set leaks, the Marker leaks too,
// and `liveAllocCount` picks it up.
//
// mir-jit only — `liveAllocCount` isn't wired into AOT.
// aot: skip
use std.test as test

@derive(Eq, Hash)
class Marker {
    pub id: i64
    init(id: i64) { this.id = id }
}

fn make_set(): Set<Marker> {
    let s = new Set<Marker>()
    s.add(new Marker(1))
    s
}

class Holder {
    init() {}
    take(_s: Set<Marker>) {}
    keep(s: Set<Marker>): Set<Marker> { s }
}

fn take_free(_s: Set<Marker>) {}

fn round_free() {
    take_free(make_set())
}

fn round_method() {
    let h = new Holder()
    h.take(make_set())
    let _ = h
}

fn round_init() {
    let h = new Holder()
    let _ = h.keep(make_set())
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
