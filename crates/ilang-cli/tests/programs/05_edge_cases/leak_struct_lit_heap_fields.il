// `class C { f: T = ... }` struct literal (`C { f: borrowed_x }`)
// has to retain `borrowed_x` so the field's +1 doesn't share an
// rc share with the source binding. Pre-fix the StructLit
// `is_heap` match in `ExprKind::StructLit` only listed Object /
// Array / Tuple / Map / Optional / Fn / Str — Enum / Set /
// Promise / Weak slipped through, so a `class C { e: E }`
// initialised from a borrowed `e: E` shared rc=1 with the source
// Local; scope-exit released to 0 and the field cascade tried to
// release a freed pointer.
//
// Set is `Box`-allocated and not __mir_alloc-tracked, so the
// probe goes through a held Tag class element. Enum is observed
// by liveAllocCount on its payload Object. Weak rides on the
// weak-table entry.
//
// mir-jit only — liveAllocCount isn't wired into AOT.
// aot: skip
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

class HoldsEnum {
    e: E
    init(e: E) { this.e = e }
}

class HoldsSet {
    s: Set<Tag>
    init(s: Set<Tag>) { this.s = s }
}

class HoldsWeak {
    w: N.weak
    init(w: N.weak) { this.w = w }
}

class HoldsPromise {
    p: Promise<N>
    init(p: Promise<N>) { this.p = p }
}

fn round_enum() {
    let e: E = E.b(new Tag(7))
    let h = new HoldsEnum(e)
    let _ = h
}

fn round_set() {
    let s = new Set<Tag>()
    s.add(new Tag(7))
    let h = new HoldsSet(s)
    let _ = h
}

fn round_weak() {
    let n = new N(7)
    let w: N.weak = n
    let h = new HoldsWeak(w)
    let _ = h
}

fn round_promise() {
    let p: Promise<N> = Promise.resolve(new N(7))
    let h = new HoldsPromise(p)
    let _ = h
}

round_enum()
round_set()
round_weak()
round_promise()

let base = test.liveAllocCount()
for _ in 0..50 {
    round_enum()
    round_set()
    round_weak()
    round_promise()
}
test.expect(test.liveAllocCount() - base, 0)
