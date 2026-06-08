// A class field of type `Set<T>` must drop both the set and
// every element it holds when the owner cell releases. Pre-fix
// the AOT-side `field_kind_tag` mapped Set → KIND_NONE, so the
// release-cascade silently skipped the field; the JIT path was
// correct via `compile::print_kind::kind_tag_of`, so this gap
// only surfaced in AOT.
//
// `Set` itself is `Box`-allocated and not tracked by
// `__mir_alloc`, so the leak probe rides on the held class
// element: if the Set's drop fires, `Tag.deinit` releases each
// Tag and `liveAllocCount` returns to baseline. If the AOT
// cascade still skips the Set field, the held Tag never sees
// its release and shows up as +50 over 50 rounds.

use std.test as test

@derive(Eq, Hash)
class Tag {
    pub n: i64
    init(n: i64) { this.n = n }
}

class Bag {
    s: Set<Tag>
    init(seed: Tag) {
        let s = new Set<Tag>()
        s.add(seed)
        this.s = s
    }
}

fn round() {
    let t = new Tag(7)
    let b = new Bag(t)
    let _ = b
    let _ = t
}

round()
let baseline = test.liveAllocCount()
for _ in 0..50 { round() }
test.expect(test.liveAllocCount() - baseline, 0)
