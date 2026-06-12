// Confirmation-round pin (第35弾): the bare-name field *write* fixed in
// 第32弾 (capture-based `this` resolution) and routed through
// `store_value_to_field` in 第33弾 must keep ARC balanced under
// repeated mutation. Two shapes not covered by the earlier pins:
//
//   1. An escaped, stateful closure that reassigns a *heap* field by
//      bare name on every call (`slot = new Box(x)`). Each call must
//      release the displaced Box; the object + closure dying at scope
//      exit drops the last one. Over a churn loop this is the strictest
//      over-/under-release detector for the bare write path.
//   2. A bare field reassignment inside `init`, after the field was
//      first set with `this.field = ...`. The first (initializing)
//      write must use `this.` — the checker requires it, which is what
//      keeps the bare path's `is_init = false` honest: by the time the
//      bare write runs the slot already holds a real Box, so releasing
//      the old value is correct (not a zeroed-slot free).
//
// Values right, deinit counts exact, churn delta = 0.

use std.test as test

let deinits: i64[] = [0]

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

class Stateful {
    slot: Box
    init() { this.slot = new Box(0) }
    updater(): fn(i64): i64 {
        fn(x: i64): i64 {
            slot = new Box(x)         // bare heap field reassign, per call
            slot.n
        }
    }
}

class InitReassign {
    slot: Box
    init(x: i64) {
        this.slot = new Box(x)        // initializing write must be `this.`
        slot = new Box(x + 100)       // bare reassign: release the just-set Box
    }
}

// Boxes per round: init Box(0) + f(1..3) each mint one (4) + the extra
// f(3) call = 5 created, all die.
fn stateful(): i64 {
    let s = new Stateful()
    let f = s.updater()
    f(1)
    f(2)
    f(3)
    f(3)
}

// Boxes per round: Box(x) (released by the bare reassign) + Box(x+100)
// (dies at scope exit) = 2.
fn initbare(): i64 {
    let h = new InitReassign(5)
    h.slot.n
}

let acc: i64[] = [0]

test.expect(stateful(), 3)
test.expect(initbare(), 105)

// Per round: stateful 5 + initbare 2 = 7 deinits.
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    acc[0] = acc[0] + stateful() + initbare()
    i = i + 1
}
test.expect(deinits[0] - bd, 700)
test.expect(test.liveAllocBytes() - d0, 0)
