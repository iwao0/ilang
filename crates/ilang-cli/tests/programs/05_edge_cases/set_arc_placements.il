// Confirms `Set<T>` ARC is uniform across the placements that the
// wrap-coerce fix (第77弾, optional_wrap_set_retain.il) did NOT touch —
// variable / Optional reassignment and an early `?`-exit that jumps
// over a live set binding. A clean-round pin: no bug here, but these
// forms had no fixture, and the recent `retain_if_heap` change makes
// it worth nailing that the set's elements drop exactly once per round
// with a flat churn delta in every position.

// expect: reassign a=2 b=1 deinits=5
// expect: try-exit a=3 b=-1 deinits=4
// expect: churn delta=0 deinits=900

use std.test as test

let deinits: i64[] = [0]

@derive(Eq, Hash)
class Box {
    pub n: i64
    init(n: i64) { this.n = n }
    deinit() { deinits[0] = deinits[0] + 1 }
}

fn makeSet(n: i64): Set<Box> {
    let s = new Set<Box>()
    s.add(new Box(n))
    s
}

// --- reassignment: the displaced set (and its box) must drop ---
fn varReassign(): i64 {
    let s = new Set<Box>()
    s.add(new Box(1))
    s = new Set<Box>()          // old set (1 box) dropped
    s.add(new Box(2))
    s.add(new Box(3))
    s.size()
}
fn optReassign(): i64 {
    let os: Set<Box>? = none
    os = some(makeSet(10))
    os = some(makeSet(20))      // old some(1 box) dropped
    match os {
        some(s) { s.size() }
        none { -1 }
    }
}

// --- `?` early-exit jumping over a live set binding ---
fn mayFail(ok: bool): Result<i64, string> {
    if ok { Result.ok(1) } else { Result.err("no") }
}
fn tryExit(ok: bool): Result<i64, string> {
    let s = new Set<Box>()       // live across the `?`
    s.add(new Box(1))
    s.add(new Box(2))
    let x = mayFail(ok)?         // err path early-returns over `s`
    Result.ok(x + s.size())
}
fn runTry(ok: bool): i64 {
    match tryExit(ok) {
        ok(v) { v }
        err(_) { -1 }
    }
}

let bd = deinits[0]
test.expect(varReassign(), 2)
test.expect(optReassign(), 1)
// varReassign: 1 (old) + 2 (new) = 3 boxes; optReassign: 10 + 20 = 2. → 5
test.expect(deinits[0] - bd, 5)
console.log("reassign a=2 b=1 deinits=5")

let bd2 = deinits[0]
test.expect(runTry(true), 3)
test.expect(runTry(false), -1)
// each runTry makes 2 boxes; both die (the early-exit sweep frees the
// set on the err path). 2 * 2 = 4
test.expect(deinits[0] - bd2, 4)
console.log("try-exit a=3 b=-1 deinits=4")

// churn every form: 3 + 2 + 2 + 2 = 9 boxes per round, flat delta.
let acc: i64[] = [0]
let d0 = test.liveAllocBytes()
let bd3 = deinits[0]
let i = 0
while i < 100 {
    acc[0] = acc[0] + varReassign() + optReassign() + runTry(true) + runTry(false)
    i = i + 1
}
test.expect(test.liveAllocBytes() - d0, 0)
test.expect(deinits[0] - bd3, 900)
console.log("churn delta=0 deinits=900")
