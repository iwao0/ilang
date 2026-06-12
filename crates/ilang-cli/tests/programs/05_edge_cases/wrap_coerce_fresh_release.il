// The `T → T?` / `T → T.weak` auto-wrap coerce mints the wrapper's
// own share of the inner. When the SOURCE was itself owned (a
// fresh value's transfer +1, or a block tail the alias/borrow
// retain bumped), that share had no owner left — one value leaked
// per call at EVERY wrap position. Round-20 fixed the annotated
// let; round-21 probes found the rest: weak targets, argument
// position, assignment, and both return paths (tail + `return`).
//
// expect: weak-join val=3 deinits=2 delta=0
// expect: ret-wrap deinits=50 delta=0
// expect: assign-wrap deinits=50
// expect: weak-assign deinits=100 delta=0
// expect: ret-stmt deinits=50 delta=0

use std.test as test
let deinits: i64[] = [0]
class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

// weak join: the strong +1 the join normalization minted must die
// at the strong→weak wrap
fn weakJoin(c: bool, a: Box, b: Box): i64 {
    let w: Box.weak = if c { a } else { b }
    match w.get() {
        some(x) { x.n }
        none { -1 }
    }
}
fn runWeak(): i64 {
    let a = new Box(1)
    let b = new Box(2)
    weakJoin(true, a, b) + weakJoin(false, a, b)
}
let bd1 = deinits[0]
let d1 = test.liveAllocBytes()
let v1 = runWeak()
console.log("weak-join val=" + v1.toString()
    + " deinits=" + (deinits[0] - bd1).toString()
    + " delta=" + (test.liveAllocBytes() - d1).toString())

// tail-position return wrap
fn retWrap(): Box? { new Box(1) }
let bd2 = deinits[0]
let d2 = test.liveAllocBytes()
let i = 0
while i < 50 {
    let o = retWrap()
    i = i + 1
}
console.log("ret-wrap deinits=" + (deinits[0] - bd2).toString()
    + " delta=" + (test.liveAllocBytes() - d2).toString())

// assignment wrap
fn assignWrap(): i64 {
    let o: Box? = none
    o = new Box(2)
    match o {
        some(b) { b.n }
        none { -1 }
    }
}
let acc: i64[] = [0]
let bd3 = deinits[0]
let j = 0
while j < 50 { acc[0] = acc[0] + assignWrap(); j = j + 1 }
console.log("assign-wrap deinits=" + (deinits[0] - bd3).toString())

// weak target through assignment
fn weakAssign(c: bool, a: Box, b: Box): i64 {
    let w: Box.weak = a
    w = if c { a } else { b }
    match w.get() {
        some(x) { x.n }
        none { -1 }
    }
}
fn runWeakAssign(): i64 {
    let a = new Box(1)
    let b = new Box(2)
    weakAssign(true, a, b) + weakAssign(false, a, b)
}
let bd4 = deinits[0]
let d4 = test.liveAllocBytes()
let k = 0
while k < 50 { acc[0] = acc[0] + runWeakAssign(); k = k + 1 }
console.log("weak-assign deinits=" + (deinits[0] - bd4).toString()
    + " delta=" + (test.liveAllocBytes() - d4).toString())

// explicit `return` wrap
fn retStmt(c: bool): Box? {
    if c { return new Box(3) }
    return none
}
let bd5 = deinits[0]
let d5 = test.liveAllocBytes()
let m = 0
while m < 50 {
    let o = retStmt(true)
    m = m + 1
}
console.log("ret-stmt deinits=" + (deinits[0] - bd5).toString()
    + " delta=" + (test.liveAllocBytes() - d5).toString())
