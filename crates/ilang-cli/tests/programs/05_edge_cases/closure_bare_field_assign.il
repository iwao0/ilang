// Regression guard: a method-internal closure that ASSIGNS to a field
// by bare name (`slot = nb`, not `this.slot = nb`) must resolve `this`
// through its captures. Bare field *reads* already did this, but the
// implicit-`this.<field>` *assignment* path in expr.rs used a raw
// `lookup_var("this").unwrap()` with no capture fallback, so any bare
// field write inside a closure panicked the compiler with
// "Option::unwrap() on a None value". The capture is already wired up
// (the assignment target is collected as a free var, which pulls `this`
// in via `name_is_this_member`); only the resolution at the store site
// was missing. Fixed by mirroring the read path's capture fallback.
//
// Covers the heap field (ARC: old value released, new value owned) and
// a primitive field (no ARC). Values must be right and ARC exact.

use std.test as test

let deinits: i64[] = [0]

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

// Heap field replaced by bare name inside a closure.
class Cell {
    slot: Box
    init(b: Box) { this.slot = b }
    setter(): fn(Box): i64 {
        fn(nb: Box): i64 {
            slot = nb            // bare field write: release old, own new
            slot.n
        }
    }
}

// Primitive field mutated by bare name inside a closure (no ARC).
class Counter {
    c: i64
    init() { this.c = 0 }
    inc(): fn(): i64 {
        fn(): i64 { c = c + 1; c }
    }
}

// Per round: Box(1) created, replaced by Box(2) -> Box(1) dies; the
// Cell dies at scope exit -> Box(2) dies. 2 deinits/round.
fn replace(): i64 {
    let c = new Cell(new Box(1))
    let s = c.setter()
    s(new Box(2))                // 2
}

fn count(): i64 {
    let k = new Counter()
    let f = k.inc()
    f() + f() + f()              // 1 + 2 + 3 = 6
}

let acc: i64[] = [0]

test.expect(replace(), 2)
test.expect(count(), 6)

// Per round: replace() = 2 deinits, count() = 0 (no Box). 2/round.
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    acc[0] = acc[0] + replace() + count()
    i = i + 1
}
test.expect(deinits[0] - bd, 200)
test.expect(test.liveAllocBytes() - d0, 0)
