// Confirmation pin (第45弾): the composite-element-wrap fixes (第36 /
// 第41) generalize to fixed-length arrays `T[N]`, which use the rc'd
// representation from 第19弾. All sound:
//
//   * Box?[2] literal       [some(box), none]   — element already wrapped
//   * Box?[2] index store    arr[i] = box        — Box -> Box? on store
//   * (Box?, Box)[2] index   arr[i] = (box, b)   — tuple slot 0 wrapped
//     store                                        via the slot hint, in a
//                                                   fixed-length buffer
//
// Dispatch through each Optional confirms the real object was stored;
// ARC is exact across the fixed-array representation.

use std.test as test

let deinits: i64[] = [0]

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

// Box?[2] literal with a some/none mix. 1 Box/round.
fn optLiteral(): i64 {
    let arr: Box?[2] = [some(new Box(1)), none]
    let a = match arr[0] {
        some(v) { v.n }
        none { -1 }
    }
    let b = match arr[1] {
        some(v) { v.n }
        none { 0 }
    }
    a + b
}

// Box?[2] index store with Box -> Box? wrap. 2 Box/round.
fn optIndexStore(): i64 {
    let arr: Box?[2] = [none, none]
    arr[0] = new Box(7)
    arr[1] = new Box(3)
    let a = match arr[0] {
        some(v) { v.n }
        none { -1 }
    }
    let b = match arr[1] {
        some(v) { v.n }
        none { -1 }
    }
    a + b
}

// (Box?, Box)[2] index store with tuple slot-0 wrap. 4 Box/round
// (two init Box(0) + Box(7) + Box(3); the displaced (none, Box0) drops).
fn tupleIndexStore(): i64 {
    let arr: (Box?, Box)[2] = [(none, new Box(0)), (none, new Box(0))]
    arr[0] = (new Box(7), new Box(3))
    match arr[0][0] {
        some(v) { v.n + arr[0][1].n }
        none { arr[0][1].n }
    }
}

let acc: i64[] = [0]

test.expect(optLiteral(), 1)
test.expect(optIndexStore(), 10)
test.expect(tupleIndexStore(), 10)

// Per round: optLiteral 1 + optIndexStore 2 + tupleIndexStore 4 = 7.
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    acc[0] = acc[0] + optLiteral() + optIndexStore() + tupleIndexStore()
    i = i + 1
}
test.expect(deinits[0] - bd, 700)
test.expect(test.liveAllocBytes() - d0, 0)
