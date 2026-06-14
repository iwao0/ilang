// Wrapping a `Set` into an `Optional` must retain the set, like every
// other heap inner. `retain_if_heap` (used by `coerce`'s `T → T?` wrap
// arm, closure captures, and the some-field path) carried a hand-copied
// heap-type list that had diverged from the canonical `MirTy::is_heap`,
// omitting `Set` / `Weak` / `Promise`. So a bare `Set<T> → Set<T>?` wrap
// never bumped the set's rc.
//
// It only showed up where the source local dies before the wrapped
// Optional is read — the RETURN position:
//
//     fn f(): Set<Box>? {
//         let s = new Set<Box>()
//         s.add(new Box(3))
//         s                      // bare wrap; s released at scope exit
//     }                          // -> freed the set; caller saw size 0
//
// A `let os: Set<Box>? = s` (source stays in scope) and an explicit
// `some(s)` both happened to read right, which is what hid it. Fix:
// `retain_if_heap` now delegates to `MirTy::is_heap`, so `Set` / `Weak`
// / `Promise` inners are retained too — `Inst::Retain` already
// dispatches to `__retain_set` / `__retain_weak` / `__retain_promise`.

// expect: optional wrap set retain ok

use std.test as test

let deinits: i64[] = [0]

@derive(Eq, Hash)
class Box {
    pub n: i64
    init(n: i64) { this.n = n }
    deinit() { deinits[0] = deinits[0] + 1 }
}

fn setSize(os: Set<Box>?): i64 {
    match os {
        some(s) { s.size() }
        none { -1 }
    }
}

// return position — the broken case: bare `Set -> Set?` wrap.
fn retOptSet(): Set<Box>? {
    let s = new Set<Box>()
    s.add(new Box(1))
    s.add(new Box(2))
    s
}

// field store — bare wrap into a `Set<Box>?` field.
class Holder {
    pub os: Set<Box>?
    init() { this.os = none }
}
fn viaField(): i64 {
    let s = new Set<Box>()
    s.add(new Box(3))
    let h = new Holder()
    h.os = s
    setSize(h.os)
}

// argument — bare wrap into a `Set<Box>?` parameter.
fn takeOptSet(os: Set<Box>?): i64 { setSize(os) }
fn viaArg(): i64 {
    let s = new Set<Box>()
    s.add(new Box(4))
    takeOptSet(s)
}

let bd = deinits[0]
test.expect(setSize(retOptSet()), 2)   // was 0 — the set was freed early
test.expect(viaField(), 1)
test.expect(viaArg(), 1)
// retOptSet: 2 boxes, viaField: 1, viaArg: 1 = 4 deinits.
test.expect(deinits[0] - bd, 4)

// churn: every set + box is freed exactly once, nothing leaks and the
// broadened retain doesn't over-retain.
let acc: i64[] = [0]
let d0 = test.liveAllocBytes()
let bd1 = deinits[0]
let i = 0
while i < 100 {
    acc[0] = acc[0] + setSize(retOptSet()) + viaField() + viaArg()
    i = i + 1
}
test.expect(test.liveAllocBytes() - d0, 0)
test.expect(deinits[0] - bd1, 400)     // 4 boxes per round, exactly

console.log("optional wrap set retain ok")
