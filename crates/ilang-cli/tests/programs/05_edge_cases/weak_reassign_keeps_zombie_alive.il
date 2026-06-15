// Sibling of weak_bind_keeps_zombie_alive.il for the REASSIGNMENT path
// (`w = strongRef`, not `let w = strongRef`). The assign lowering shared
// the let-binding's "coerced => fresh" shortcut and skipped the weak
// retain on a `StrongToWeak`, freeing the zombie box while the weak
// still pointed at it.
//
// Deterministic in normal mode via slot reuse: without the retain the
// recycled slot makes `w.get()` upgrade to a bogus `some`.
//
// expect: ok none

class Tag {
    v: i64
    init(x: i64) { this.v = x }
}
class Box {
    w: Tag.weak
    init() {}
}

fn fill(b: Box) {
    let d = new Tag(0)
    let w: Tag.weak = d
    let n = new Tag(111)
    w = n
    b.w = w
}

let b = new Box()
fill(b)
let filler: Tag[] = []
let i = 0
while i < 8 {
    filler.push(new Tag(999))
    i = i + 1
}
match b.w.get() {
    some(t) { console.log(`BAD some v=${t.v}`) }
    none { console.log("ok none") }
}
