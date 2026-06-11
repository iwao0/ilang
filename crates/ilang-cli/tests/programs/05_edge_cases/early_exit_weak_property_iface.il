// Round-17 surface pins — shapes the early-exit sweeps and ARC
// machinery must keep exact, none of which had a dedicated fixture:
// - a weak field's registration inside a fn left via early `return`
//   (the sweep must not touch the strong count: deinits stay 0 and
//   the target stays readable)
// - property getter/setter BODIES with early returns and heap
//   locals (accessor bodies are fn bodies; the sweep applies)
// - interface-typed fresh arrays pierced by a `return` out of a
//   for-in (vtable dispatch + the fresh-iterable release + element
//   borrows: every element deinits exactly once per call)
// - string-repr enums and @flags enums round-tripping
//
// expect: weak-early-return ok n=5 deinits=0
// expect: property strings=0 sum-ok
// expect: iface deinits=400 delta-ok
// expect: repr=BLUE match=2
// expect: flags has-read=true has-write=false

use std.test as test

let deinits: i64[] = [0]
class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}
class Owner {
    peer: Box.weak
    init(b: Box) { this.peer = b }
}
fn peek(strong: Box, flag: bool): i64 {
    let o = new Owner(strong)
    if flag { return strong.n }
    0
}
let b = new Box(5)
let before = deinits[0]
let i = 0
let acc: i64[] = [0]
while i < 200 { acc[0] = acc[0] + peek(b, true); i = i + 1 }
console.log("weak-early-return ok n=" + b.n.toString()
    + " deinits=" + (deinits[0] - before).toString())

class Temp {
    _raw: i64
    init() { this._raw = 0 }
    pub get celsius(): i64 {
        let tag: string = "deg" + this._raw.toString()
        if this._raw > 100 { return 100 + tag.length }
        this._raw
    }
    pub set celsius(v: i64) {
        let note: string = "set" + v.toString()
        if v > 1000 { return }
        this._raw = v + note.length * 0
    }
}
let t = new Temp()
let s1 = test.liveStringCount()
let j = 0
while j < 200 {
    t._raw = j
    acc[0] = acc[0] + t.celsius
    t.celsius = j * 2
    j = j + 1
}
let ds = test.liveStringCount() - s1
test.expectTrue(acc[0] > 0)
console.log("property strings=" + ds.toString() + " sum-ok")

interface Shape {
    area(): i64
}
class Sq: Shape {
    s: i64
    init(v: i64) { this.s = v }
    deinit() { deinits[0] = deinits[0] + 1 }
    pub area(): i64 { this.s * this.s }
}
class Ci: Shape {
    r: i64
    init(v: i64) { this.r = v }
    deinit() { deinits[0] = deinits[0] + 1 }
    pub area(): i64 { this.r * this.r * 3 }
}
fn firstBig(shapes: Shape[], limit: i64): i64 {
    for s in shapes {
        let a = s.area()
        if a > limit { return a }
    }
    return -1
}
fn freshShapes(k: i64): Shape[] {
    let out: Shape[] = []
    out.push(new Sq(k))
    out.push(new Ci(k + 1))
    out
}
let b2 = test.liveAllocBytes()
let before2 = deinits[0]
let m = 1
while m < 201 {
    acc[0] = acc[0] + firstBig(freshShapes(m), 0)
    m = m + 1
}
let d2 = test.liveAllocBytes() - b2
test.expectTrue(d2 < 256)
console.log("iface deinits=" + (deinits[0] - before2).toString() + " delta-ok")

enum Color: string {
    red = "RED"
    blue = "BLUE"
}
let c = Color.blue
let picked = match c {
    red { 1 }
    blue { 2 }
}
console.log("repr=" + (c as string) + " match=" + picked.toString())

@flags
enum Perm: u32 {
    read = 1
    write = 2
    exec = 4
}
let p = Perm.read | Perm.exec
console.log("flags has-read=" + ((p & Perm.read) != (0 as u32)).toString()
    + " has-write=" + ((p & Perm.write) != (0 as u32)).toString())
