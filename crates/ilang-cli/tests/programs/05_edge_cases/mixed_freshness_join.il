// Mixed-freshness joins — an `if` / `match` / `if let` whose
// branches disagree on ownership (one arm builds a fresh value,
// the other hands back a param / binding). The join now NORMALIZES
// to owned: non-owned heap results get a join-side Retain, every
// join value uniformly owns its +1, and the consumer treats the
// expression as fresh. Before this, the fresh branch was retained
// a second time by the consumer and leaked one object per
// evaluation through that branch (if AND match AND if-let — found
// by round-20 probes; the `?`-on-Optional work had just fixed the
// equivalent under-retain in the `?` desugar).
//
// Also pinned: the `T → T?` auto-wrap of a FRESH source releases
// the inner's transfer +1 in ARGUMENT position too (the stmt-let
// path was fixed earlier; `takeOpt(new Box(42))` still leaked).
//
// expect: if val=51 deinits=2 delta=0
// expect: match val=51 deinits=2 delta=0
// expect: iflet val=51 deinits=2 delta=0
// expect: arg-wrap val=42 deinits=1
// expect: churn delta=0 deinits=400

use std.test as test
let deinits: i64[] = [0]
class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

fn mixedIf(c: bool, fallback: Box): i64 {
    let b = if c { new Box(1) } else { fallback }
    b.n
}
fn runIf(): i64 {
    let fb = new Box(50)
    mixedIf(true, fb) + mixedIf(false, fb)
}
let bd1 = deinits[0]
let d1 = test.liveAllocBytes()
let v1 = runIf()
console.log("if val=" + v1.toString()
    + " deinits=" + (deinits[0] - bd1).toString()
    + " delta=" + (test.liveAllocBytes() - d1).toString())

fn mixedMatch(k: i64, fallback: Box): i64 {
    let b = match k {
        0 { new Box(1) }
        _ { fallback }
    }
    b.n
}
fn runMatch(): i64 {
    let fb = new Box(50)
    mixedMatch(0, fb) + mixedMatch(7, fb)
}
let bd2 = deinits[0]
let d2 = test.liveAllocBytes()
let v2 = runMatch()
console.log("match val=" + v2.toString()
    + " deinits=" + (deinits[0] - bd2).toString()
    + " delta=" + (test.liveAllocBytes() - d2).toString())

fn mixedIfLet(o: i64?, fallback: Box): i64 {
    let b = if let some(k) = o { new Box(k) } else { fallback }
    b.n
}
fn runIfLet(): i64 {
    let fb = new Box(50)
    mixedIfLet(some(1), fb) + mixedIfLet(none, fb)
}
let bd3 = deinits[0]
let d3 = test.liveAllocBytes()
let v3 = runIfLet()
console.log("iflet val=" + v3.toString()
    + " deinits=" + (deinits[0] - bd3).toString()
    + " delta=" + (test.liveAllocBytes() - d3).toString())

// fresh arg auto-wrapped to T?
fn takeOpt(o: Box?): i64 {
    match o {
        some(b) { b.n }
        none { -1 }
    }
}
let bd4 = deinits[0]
let v4 = takeOpt(new Box(42))
console.log("arg-wrap val=" + v4.toString()
    + " deinits=" + (deinits[0] - bd4).toString())

let acc: i64[] = [0]
let d5 = test.liveAllocBytes()
let bd5 = deinits[0]
let i = 0
while i < 100 {
    acc[0] = acc[0] + runIf() + runMatch()
    i = i + 1
}
console.log("churn delta=" + (test.liveAllocBytes() - d5).toString()
    + " deinits=" + (deinits[0] - bd5).toString())
