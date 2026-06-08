// `enum E { v: (Set<T>) }` ctor with a non-fresh Set argument
// must retain it so the enum cell owns its own +1 — otherwise
// the cell's release-cascade decrements an rc the caller never
// bumped and the held element double-drops. Pre-fix
// `lower_enum_ctor`'s `needs_retain` set only covered Object /
// Array / Tuple / Map / Optional / Fn / Str; Set / Enum /
// Promise / Weak were silently missing. Enum-in-Enum (the
// `Tree.node(_, _, inner)` case) was the original repro; Set
// follows the same shape and is pinned here so the parallel
// gaps don't regress.
//
// `Set` itself is `Box`-allocated and not tracked by
// `__mir_alloc`, so the leak rides on the held class element:
// if the ctor fails to retain, scope-exit releases the Local
// `s` to rc=0 (freeing the set + dropping its Tag), and the
// enum cell's subsequent cascade tries to release a freed
// pointer — manifests as +50 or a crash.

use std.test as test

@derive(Eq, Hash)
class Tag {
    pub n: i64
    init(n: i64) { this.n = n }
}

enum WithSet {
    nothing
    held: (Set<Tag>)
}

fn round() {
    let s = new Set<Tag>()
    s.add(new Tag(7))
    let e = WithSet.held(s)
    let _ = e
    let _ = s
}

round()
let baseline = test.liveAllocCount()
for _ in 0..50 { round() }
test.expect(test.liveAllocCount() - baseline, 0)
