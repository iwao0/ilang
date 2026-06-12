// Round-22 finds, all pre-existing:
// 1. `h.opt = new Box(1)` (field typed Box?) — AssignField's own
//    wrap coerce never released the fresh source (the sixth wrap
//    position, after round-21 fixed the other five).
// 2. `takeOpt(h.b)` — a BORROWED source wrapped at an argument
//    position mints a fresh cell the post-release decision didn't
//    see (it keyed on the source expression's freshness only):
//    the wrapper cell + its retained inner share leaked per call.
// 3. `let o: Box? = h.b` / `= { ...; b }` — the binding retain
//    keyed on the SOURCE's freshness and retained the freshly
//    minted wrapper a second time. Same in assignment position.
// 4. `let m: Map<string, Box?> = {"a": none, ...}` — the checker
//    accepted what the lowering rejected ("no coercion from
//    Map<string, ()?>"): map literals now build against the
//    declared K/V (none adopts V, values wrap, nested literals
//    get hints).
//
// expect: field-wrap val=2 deinits=2 delta=0
// expect: arg-borrow-wrap deinits=100
// expect: borrow-wrap val=33 deinits=1 delta=0
// expect: block-wrap val=13 deinits=1 delta=0
// expect: hinted-map val=107 deinits=2 delta=0
// expect: churn delta=0 deinits=600

use std.test as test
let deinits: i64[] = [0]
class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}
class Holder {
    opt: Box?
    init() { this.opt = none }
}
class H {
    b: Box
    init(x: Box) { this.b = x }
}

// 1. field wrap
fn fieldWrap(): i64 {
    let h = new Holder()
    h.opt = new Box(1)
    h.opt = new Box(2)
    match h.opt {
        some(b) { b.n }
        none { -1 }
    }
}
let bd1 = deinits[0]
let d1 = test.liveAllocBytes()
let v1 = fieldWrap()
console.log("field-wrap val=" + v1.toString()
    + " deinits=" + (deinits[0] - bd1).toString()
    + " delta=" + (test.liveAllocBytes() - d1).toString())

// 2. borrowed arg wrap
fn takeOpt(o: Box?): i64 {
    match o {
        some(b) { b.n }
        none { -1 }
    }
}
fn argBorrowWrap(): i64 {
    let h = new H(new Box(21))
    takeOpt(h.b)
}
let bd2 = deinits[0]
let acc: i64[] = [0]
let i = 0
while i < 100 { acc[0] = acc[0] + argBorrowWrap(); i = i + 1 }
console.log("arg-borrow-wrap deinits=" + (deinits[0] - bd2).toString())

// 3a. borrowed let wrap
fn borrowWrap(h: H): i64 {
    let o: Box? = h.b
    match o {
        some(b) { b.n }
        none { -1 }
    }
}
fn runBorrow(): i64 {
    let h = new H(new Box(11))
    let a = borrowWrap(h)
    let b = borrowWrap(h)
    a + b + h.b.n
}
let bd3 = deinits[0]
let d3 = test.liveAllocBytes()
let v3 = runBorrow()
console.log("borrow-wrap val=" + v3.toString()
    + " deinits=" + (deinits[0] - bd3).toString()
    + " delta=" + (test.liveAllocBytes() - d3).toString())

// 3b. block-tail-retained let wrap
fn blockWrap(): i64 {
    let o: Box? = {
        let b = new Box(13)
        b
    }
    match o {
        some(b) { b.n }
        none { -1 }
    }
}
let bd4 = deinits[0]
let d4 = test.liveAllocBytes()
let v4 = blockWrap()
console.log("block-wrap val=" + v4.toString()
    + " deinits=" + (deinits[0] - bd4).toString()
    + " delta=" + (test.liveAllocBytes() - d4).toString())

// 4. annotated map literal with none / fresh / some values
fn build(): i64 {
    let m: Map<string, Box?> = {"a": none, "b": new Box(7), "c": some(new Box(8))}
    let x = match m.get("b") {
        some(o) {
            match o {
                some(b) { b.n }
                none { -2 }
            }
        }
        none { -1 }
    }
    let y = match m.get("a") {
        some(o) {
            match o {
                some(b) { b.n }
                none { 100 }
            }
        }
        none { -1 }
    }
    x + y
}
let bd5 = deinits[0]
let d5 = test.liveAllocBytes()
let v5 = build()
console.log("hinted-map val=" + v5.toString()
    + " deinits=" + (deinits[0] - bd5).toString()
    + " delta=" + (test.liveAllocBytes() - d5).toString())

let d6 = test.liveAllocBytes()
let bd6 = deinits[0]
let j = 0
while j < 100 {
    acc[0] = acc[0] + fieldWrap() + runBorrow() + blockWrap() + build()
    j = j + 1
}
console.log("churn delta=" + (test.liveAllocBytes() - d6).toString()
    + " deinits=" + (deinits[0] - bd6).toString())
