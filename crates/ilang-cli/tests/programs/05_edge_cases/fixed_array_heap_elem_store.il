// Fixed-length arrays (`T[N]`) with HEAP elements — full ARC
// support. Ownership model:
// - a FRESH literal's binding owns the header-less buffer plus one
//   share per element; scope exit / early exit releases both
// - a class field owns its buffer; init transfers a fresh literal's
//   buffer by pointer, a non-fresh source is value-copied
//   (`$array.copyFixed`) so two owners never share storage
// - aliases (`let v = p.items`, fn params) are borrows — never swept
// - the object-drop cascade decodes a packed tag
//   (`KIND_FIXED_BASE + len*16 + elem_kind`) to release the field
//
// expect: local delta=0 deinits=400
// expect: overwrite deinits=150 values-ok
// expect: escape n=8
// expect: field-drop deinits=400
// expect: field-overwrite deinits=2 now=3,4
// expect: copy-assign deinits=2 p0=10 q0=99
// expect: alias-read deinits=0 sum=3000
// expect: param-borrow sum=7 deinits=2
// expect: early-return deinits=100
// expect: surface len=2 forin=11 pack-drop=1002

use std.test as test
let deinits: i64[] = [0]
class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

// ── fresh literal local: scope exit frees elements + buffer ──
fn fixedLocal(k: i64): i64 {
    let arr: Box[2] = [new Box(k), new Box(k + 1)]
    arr[0].n + arr[1].n
}
let acc: i64[] = [0]
let i = 0
let before1 = deinits[0]
let b1 = test.liveAllocBytes()
while i < 200 { acc[0] = acc[0] + fixedLocal(i); i = i + 1 }
let d1 = test.liveAllocBytes() - b1
console.log("local delta=" + d1.toString()
    + " deinits=" + (deinits[0] - before1).toString())

// ── element overwrite: old element released exactly once ──
fn overwrite(k: i64): i64 {
    let arr: Box[2] = [new Box(k), new Box(k + 1)]
    arr[0] = new Box(k + 100)
    arr[0].n + arr[1].n
}
let before2 = deinits[0]
let j = 0
while j < 50 { acc[0] = acc[0] + overwrite(j); j = j + 1 }
test.expectTrue(acc[0] > 0)
console.log("overwrite deinits=" + (deinits[0] - before2).toString()
    + " values-ok")

// ── element read outlives the per-call storage ──
fn escape(k: i64): Box {
    let arr: Box[2] = [new Box(k), new Box(k + 1)]
    let keep = arr[1]
    keep
}
let g = escape(7)
console.log("escape n=" + g.n.toString())

// ── field: init transfer + object-drop cascade ──
class Pack {
    items: Box[2]
    init(a: Box, b: Box) { this.items = [a, b] }
}
let before3 = deinits[0]
let w = 0
while w < 200 {
    let _p = new Pack(new Box(w), new Box(w + 1))
    w = w + 1
}
console.log("field-drop deinits=" + (deinits[0] - before3).toString())

// ── field overwrite: old region (elements + buffer) freed ──
let p = new Pack(new Box(10), new Box(20))
let before4 = deinits[0]
p.items = [new Box(3), new Box(4)]
console.log("field-overwrite deinits=" + (deinits[0] - before4).toString()
    + " now=" + p.items[0].n.toString() + "," + p.items[1].n.toString())

// ── non-fresh field assign is a value copy, not an alias ──
p.items[0] = new Box(10)
p.items[1] = new Box(20)
let q = new Pack(new Box(1), new Box(2))
let before5 = deinits[0]
q.items = p.items
q.items[0] = new Box(99)
console.log("copy-assign deinits=" + (deinits[0] - before5).toString()
    + " p0=" + p.items[0].n.toString()
    + " q0=" + q.items[0].n.toString())

// ── field alias read is a borrow — never swept ──
fn aliasRead(t: Pack): i64 {
    let view = t.items
    view[0].n + view[1].n
}
let before6 = deinits[0]
let s: i64[] = [0]
let k = 0
while k < 100 { s[0] = s[0] + aliasRead(p); k = k + 1 }
console.log("alias-read deinits=" + (deinits[0] - before6).toString()
    + " sum=" + s[0].toString())

// ── param pass is a borrow; callee frees nothing ──
fn sum2(a: Box[2]): i64 { a[0].n + a[1].n }
fn passParam(): i64 {
    let arr: Box[2] = [new Box(3), new Box(4)]
    sum2(arr)
}
let before7 = deinits[0]
let r = passParam()
console.log("param-borrow sum=" + r.toString()
    + " deinits=" + (deinits[0] - before7).toString())

// ── early return sweeps the owned local ──
fn earlyFixed(k: i64): i64 {
    let arr: Box[2] = [new Box(k), new Box(k + 1)]
    if k >= 0 { return arr[0].n }
    0
}
let before8 = deinits[0]
let v = 0
while v < 50 { acc[0] = acc[0] + earlyFixed(v); v = v + 1 }
console.log("early-return deinits=" + (deinits[0] - before8).toString())

// ── surface ops: length / for-in / drop cascade exactness ──
let a2: Box[2] = [new Box(5), new Box(6)]
let f: i64[] = [0]
for b in a2 { f[0] = f[0] + b.n }
class Tagged {
    items: Box[2]
    init(x: Box, y: Box) { this.items = [x, y] }
    deinit() { deinits[0] = deinits[0] + 1000 }
}
fn dropTagged() {
    let _t = new Tagged(new Box(1), new Box(2))
}
let before9 = deinits[0]
dropTagged()
console.log("surface len=" + a2.length.toString()
    + " forin=" + f[0].toString()
    + " pack-drop=" + (deinits[0] - before9).toString())
