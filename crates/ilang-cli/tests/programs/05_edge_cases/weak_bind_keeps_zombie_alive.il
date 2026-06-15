// `let w: T.weak = strongRef` must retain a weak share so the box
// survives as a zombie (strong rc 0, memory valid) for as long as the
// weak lives — that is what lets `WeakUpgrade` peek the dead rc and
// answer `none`.
//
// Regression: the binding lowered `StrongToWeak` as if it were a wrap
// coerce that mints a fresh +1 cell, so it skipped the binding retain
// while the scope-exit release still fired. The weak local was released
// once but never retained → the zombie box was freed while the weak
// still pointed at it (a use-after-free, normally masked because the
// freed slot reads back 0, but real).
//
// This pins it deterministically without ILANG_HEAP_GUARD: after the
// dangling weak is created, a burst of same-shape allocations reuses the
// freed slot and writes a positive rc there. Without the retain,
// `w.get()` reads that reused slot and wrongly upgrades to `some`; with
// it, the box stays alive with rc 0 and `w.get()` is `none`.
//
// expect: ok none

class Tag {
    v: i64
    init(x: i64) { this.v = x }
}

fn mkWeak(): Tag.weak {
    let n = new Tag(111)
    let w: Tag.weak = n
    w
}

let w = mkWeak()
let filler: Tag[] = []
let i = 0
while i < 8 {
    filler.push(new Tag(999))
    i = i + 1
}
match w.get() {
    some(t) { console.log(`BAD some v=${t.v}`) }
    none { console.log("ok none") }
}
