// Round-23 breadth sweep — NO new bugs (third clean round). Pins
// the shapes probed: entries() tuple iteration with an early
// return, Set churn with @derive(Eq, Hash) keys, weak-upgrade
// churn, template literals interpolating joins and fresh strings,
// closures stored in (and reassigned on) object fields, deep
// split/join string churn, vtable dispatch with heap-returning
// overrides, and an escaping closure keeping its mutable capture
// alive. Exact deinit counts; churn deltas zero.
//
// expect: entries val=7 deinits=3 delta=0
// expect: set val=11 deinits=5 delta=0
// expect: weak-churn val=70 deinits=1 delta=0
// expect: tpl val=25 deinits=1
// expect: closure-field val=121 deinits=1 delta=0
// expect: inherit val=6 deinits=1 delta=0
// expect: counter val=6 deinits=1 delta=0
// expect: churn delta=0 deinits=1300

use std.test as test
let deinits: i64[] = [0]
class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

// entries() tuple iteration + early return
fn sumUntil(m: Map<string, Box>, stop: i64): i64 {
    let total: i64[] = [0]
    for e in m.entries() {
        let (k, v) = e
        if v.n == stop { return total[0] }
        total[0] = total[0] + v.n
    }
    return total[0]
}
fn runEntries(): i64 {
    let m: Map<string, Box> = {"a": new Box(1), "b": new Box(2), "c": new Box(3)}
    sumUntil(m, 99) + sumUntil(m, 2)
}
let bd1 = deinits[0]
let d1 = test.liveAllocBytes()
let v1 = runEntries()
console.log("entries val=" + v1.toString()
    + " deinits=" + (deinits[0] - bd1).toString()
    + " delta=" + (test.liveAllocBytes() - d1).toString())

// Set with @derive keys: add / dup / has / delete / iterate
@derive(Eq, Hash)
class Key {
    id: i64
    init(v: i64) { this.id = v }
    deinit() { deinits[0] = deinits[0] + 1 }
}
fn setChurn(k: i64): i64 {
    let s = new Set<Key>()
    s.add(new Key(k))
    s.add(new Key(k + 1))
    s.add(new Key(k))
    let had = s.has(new Key(k))
    s.delete(new Key(k))
    let total: i64[] = [0]
    for e in s.values() {
        total[0] = total[0] + e.id
    }
    if had { total[0] } else { -1 }
}
let bd2 = deinits[0]
let d2 = test.liveAllocBytes()
let v2 = setChurn(10)
console.log("set val=" + v2.toString()
    + " deinits=" + (deinits[0] - bd2).toString()
    + " delta=" + (test.liveAllocBytes() - d2).toString())

// weak upgrade churn — each get() optional is reclaimed
fn weakChurn(b: Box): i64 {
    let w: Box.weak = b
    let total: i64[] = [0]
    let i = 0
    while i < 10 {
        match w.get() {
            some(x) { total[0] = total[0] + x.n }
            none { total[0] = -999 }
        }
        i = i + 1
    }
    total[0]
}
fn runWeak(): i64 {
    let b = new Box(7)
    weakChurn(b)
}
let bd3 = deinits[0]
let d3 = test.liveAllocBytes()
let v3 = runWeak()
console.log("weak-churn val=" + v3.toString()
    + " deinits=" + (deinits[0] - bd3).toString()
    + " delta=" + (test.liveAllocBytes() - d3).toString())

// template literal interpolating joins and fresh strings
fn tpl(c: bool, b: Box): string {
    let tag = if c { "hot-" + b.n.toString() } else { "cold" }
    `[${tag}] n=${b.n} ${if c { "Y" } else { "N" }}`
}
fn runTpl(): i64 {
    let b = new Box(5)
    tpl(true, b).length + tpl(false, b).length
}
let warm = runTpl()
let bd4 = deinits[0]
let v4 = runTpl()
console.log("tpl val=" + v4.toString()
    + " deinits=" + (deinits[0] - bd4).toString())

// closure stored in a field, then reassigned (old capture dies)
class CB {
    f: fn(i64): i64
    init(g: fn(i64): i64) { this.f = g }
}
fn closureField(): i64 {
    let base = new Box(100)
    let cb = new CB(fn(x: i64): i64 { x + base.n })
    let r1 = cb.f(1)
    cb.f = fn(x: i64): i64 { x * 2 }
    let r2 = cb.f(10)
    r1 + r2
}
let bd5 = deinits[0]
let d5 = test.liveAllocBytes()
let v5 = closureField()
console.log("closure-field val=" + v5.toString()
    + " deinits=" + (deinits[0] - bd5).toString()
    + " delta=" + (test.liveAllocBytes() - d5).toString())

// vtable dispatch, heap-returning override, super with heap arg
class Animal {
    tag: Box
    init(t: Box) { this.tag = t }
    speak(): string { "..." + this.tag.n.toString() }
}
class Dog: Animal {
    init(t: Box) { super.init(t) }
    override speak(): string { "woof-" + this.tag.n.toString() }
}
fn runInherit(): i64 {
    let a: Animal = new Dog(new Box(3))
    a.speak().length
}
let bd6 = deinits[0]
let d6 = test.liveAllocBytes()
let v6 = runInherit()
console.log("inherit val=" + v6.toString()
    + " deinits=" + (deinits[0] - bd6).toString()
    + " delta=" + (test.liveAllocBytes() - d6).toString())

// escaping closure keeps its mutable capture alive
fn mkCounter(): fn(): i64 {
    let b = new Box(0)
    fn(): i64 {
        b.n = b.n + 1
        b.n
    }
}
fn runCounter(): i64 {
    let c = mkCounter()
    c() + c() + c()
}
let bd7 = deinits[0]
let d7 = test.liveAllocBytes()
let v7 = runCounter()
console.log("counter val=" + v7.toString()
    + " deinits=" + (deinits[0] - bd7).toString()
    + " delta=" + (test.liveAllocBytes() - d7).toString())

// combined churn
fn strOps(k: i64): i64 {
    let s = "a,b,c," + k.toString()
    let parts = s.split(",")
    let joined = parts.join("-")
    parts.length + joined.length
}
let acc: i64[] = [0]
let d8 = test.liveAllocBytes()
let bd8 = deinits[0]
let i = 0
while i < 100 {
    acc[0] = acc[0] + runEntries() + setChurn(i) + runWeak()
        + runTpl() + closureField() + runInherit() + strOps(i) + runCounter()
    i = i + 1
}
console.log("churn delta=" + (test.liveAllocBytes() - d8).toString()
    + " deinits=" + (deinits[0] - bd8).toString())
