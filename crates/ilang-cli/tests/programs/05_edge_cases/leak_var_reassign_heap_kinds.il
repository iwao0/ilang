// Variable reassignment (`x = new_value`) must release whatever
// the slot held before the new value lands. Pre-fix the `is_heap`
// match in `ExprKind::Assign` only listed `MirTy::Object`, so a
// `let xs: i64[] = [1,2,3]; xs = [4,5]` leaked every prior array,
// and the parallel gap covered every other rc-slot kind — Tuple /
// Map / Set / Optional / Fn / Str / Enum and (separately pinned)
// Promise / Weak. After the fix the lower routes the slot through
// `MirTy::is_heap`, which is the authoritative kind list shared
// with `release_field_by_kind` (runtime/cascade.rs) and the
// scope-exit `needs_release` sweep.
//
// `Set` and `Map` themselves are `Box`-allocated and not tracked by
// `__mir_alloc`, so the leak rides on the held class element
// (`Tag`). Every round leaks one `Tag` per missed release.

use std.test as test

@derive(Eq, Hash)
class Tag {
    pub n: i64
    init(n: i64) { this.n = n }
}

class N {
    v: i64
    init(x: i64) { this.v = x }
}

enum E {
    a
    b: (Tag)
}

fn reassign_object() {
    let o: N = new N(0)
    let i: i64 = 0
    while i < 20 {
        o = new N(i)
        i = i + 1
    }
}

fn reassign_array() {
    let a: Tag[] = [new Tag(0)]
    let i: i64 = 0
    while i < 20 {
        a = [new Tag(i), new Tag(i + 1)]
        i = i + 1
    }
}

fn reassign_tuple() {
    let t: (Tag, i64) = (new Tag(0), 0)
    let i: i64 = 0
    while i < 20 {
        t = (new Tag(i), i)
        i = i + 1
    }
}

fn reassign_map() {
    let m: Map<i64, Tag> = new Map<i64, Tag>()
    let i: i64 = 0
    while i < 20 {
        m = new Map<i64, Tag>()
        m.set(0, new Tag(i))
        i = i + 1
    }
}

fn reassign_set() {
    let s: Set<Tag> = new Set<Tag>()
    let i: i64 = 0
    while i < 20 {
        s = new Set<Tag>()
        s.add(new Tag(i))
        i = i + 1
    }
}

fn reassign_optional() {
    let opt: Tag? = none
    let i: i64 = 0
    while i < 20 {
        opt = some(new Tag(i))
        i = i + 1
    }
}

fn reassign_enum() {
    let e: E = E.a
    let i: i64 = 0
    while i < 20 {
        e = E.b(new Tag(i))
        i = i + 1
    }
}

// Warm up to amortise any one-shot allocations.
reassign_object()
reassign_array()
reassign_tuple()
reassign_map()
reassign_set()
reassign_optional()
reassign_enum()

let base = test.liveAllocCount()
for _ in 0..50 {
    reassign_object()
    reassign_array()
    reassign_tuple()
    reassign_map()
    reassign_set()
    reassign_optional()
    reassign_enum()
}
test.expect(test.liveAllocCount() - base, 0)

// Strings are tracked separately by liveStringCount.
fn reassign_str() {
    let s: string = ""
    let i: i64 = 0
    while i < 20 {
        s = "x-" + i.toString()
        i = i + 1
    }
}

reassign_str()
let base_str = test.liveStringCount()
for _ in 0..50 { reassign_str() }
test.expectTrue(test.liveStringCount() - base_str < 64)
