// The last three fixed-length heap-element array restrictions,
// lifted once the values became ordinary rc'd arrays:
// - RETURNING one — read semantics, like let/params: returning a
//   local transfers it, returning a field hands out a share
// - whole-binding REASSIGNMENT — rebinds to a share (the old
//   array's own share drops, exactly like other heap lets)
// - `Array.fill` — `$array.fillCopy` gives every slot its own
//   shallow copy (plain fill would alias one value per cell)
// Plus the regression this work uncovered: a FRESH source wrapped
// by the `let o: T? = f()` auto-coerce leaked one value per call
// (heap T in general, not just fixed arrays).
//
// expect: ret-local val=3 deinits=2
// expect: ret-share team0=99
// expect: ret-cell val=1 deinits=2
// expect: reassign val=50 deinits=5
// expect: fill val=7 deinits=3
// expect: coerce delta=0 deinits=200
// expect: nonfixed coerce delta=0 deinits=100

use std.test as test
let deinits: i64[] = [0]
class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}
class Team {
    members: Box[2]
    init(a: Box, b: Box) { this.members = [a, b] }
}

fn make(): Box[2] {
    let a: Box[2] = [new Box(1), new Box(2)]
    a
}

// returning a local — caller owns the array
fn useMake(): i64 {
    let r = make()
    r[0].n + r[1].n
}
let b1 = deinits[0]
console.log("ret-local val=" + useMake().toString()
    + " deinits=" + (deinits[0] - b1).toString())

// returning a field — a share, writes are visible (read semantics)
fn members(t: Team): Box[2] { t.members }
let team = new Team(new Box(10), new Box(20))
let view = members(team)
view[0] = new Box(99)
console.log("ret-share team0=" + team.members[0].n.toString())

// fresh return value transfers into a cell
fn intoCell(): i64 {
    let o = some(make())
    match o {
        some(arr) { arr[0].n }
        none { -1 }
    }
}
let b3 = deinits[0]
console.log("ret-cell val=" + intoCell().toString()
    + " deinits=" + (deinits[0] - b3).toString())

// reassignment rebinds to a share
fn reassign(): i64 {
    let x: Box[2] = [new Box(3), new Box(4)]
    let y: Box[2] = [new Box(5), new Box(6)]
    x = y
    y[0] = new Box(50)
    x[0].n
}
let b4 = deinits[0]
console.log("reassign val=" + reassign().toString()
    + " deinits=" + (deinits[0] - b4).toString())

// fill stores an independent copy per slot
fn fillCopy(): i64 {
    let a: Box[2] = [new Box(7), new Box(8)]
    let xs: Box[2][] = [a, a]
    xs.fill(a)
    xs[0][0] = new Box(70)
    xs[1][0].n
}
let b5 = deinits[0]
console.log("fill val=" + fillCopy().toString()
    + " deinits=" + (deinits[0] - b5).toString())

// fresh source through the let auto-coerce — no leak
fn viaCoerce(k: i64): i64 {
    let o: Box[2]? = make()
    match o {
        some(arr) { arr[0].n }
        none { -1 }
    }
}
let acc: i64[] = [0]
let b6 = test.liveAllocBytes()
let bd = deinits[0]
let i = 0
while i < 100 { acc[0] = acc[0] + viaCoerce(i); i = i + 1 }
console.log("coerce delta=" + (test.liveAllocBytes() - b6).toString()
    + " deinits=" + (deinits[0] - bd).toString())

// the same shape with a plain class value (regression for the
// pre-existing leak)
fn makeBox(): Box { new Box(1) }
fn viaCoerceBox(k: i64): i64 {
    let o: Box? = makeBox()
    match o {
        some(b) { b.n }
        none { -1 }
    }
}
let b7 = test.liveAllocBytes()
let bd2 = deinits[0]
let j = 0
while j < 100 { acc[0] = acc[0] + viaCoerceBox(j); j = j + 1 }
console.log("nonfixed coerce delta=" + (test.liveAllocBytes() - b7).toString()
    + " deinits=" + (deinits[0] - bd2).toString())
