// Mutating captures (closure assigns a captured var) go through a
// shared 1-element heap cell. The ownership model: the scope owns the
// creation +1, each capturing closure retains its own share at
// MakeClosure, and `__release_closure`'s capture cascade drops it —
// whichever side dies last frees the cell, which cascades into the
// inner value. Cell capture slots used to be skipped in the capture
// registration ("leak for now"), so every mutating capture leaked the
// cell plus its occupant (56–80 bytes per closure).

use std.test as test

class Box {
    n: i64
    init(x: i64) { this.n = x }
}

// scope-local churn: cell + contents must free every lap
fn churnObject() {
    let i = 0
    while i < 200 {
        let current = new Box(0)
        let setter = fn(v: i64) { current = new Box(v) }
        setter(i)
        test.expect(current.n, i)
        i = i + 1
    }
}

fn churnPrim() {
    let i = 0
    while i < 200 {
        let cnt = 0
        let bump = fn() { cnt = cnt + 1 }
        bump()
        test.expect(cnt, 1)
        i = i + 1
    }
}

// constructed but never called — the cell still frees
fn churnNoCall() {
    let i = 0
    while i < 200 {
        let current = new Box(0)
        let setter = fn(v: i64) { current = new Box(v) }
        let _ = setter
        i = i + 1
    }
}

// closure outlives the scope: the cell survives on the closure's
// share after the scope drops its own
fn makeCounter(): fn(): i64 {
    let count = 0
    fn(): i64 { count = count + 1; count }
}

// two closures share one cell — a write through one is visible to
// the other, and the cell frees only after both die
fn makePair(): (fn(), fn(): i64) {
    let current = new Box(0)
    let set = fn() { current = new Box(99) }
    let get = fn(): i64 { current.n }
    (set, get)
}

let c = makeCounter()
test.expect(c(), 1)
test.expect(c(), 2)

let pair = makePair()
let setF = pair[0]
let getF = pair[1]
test.expect(getF(), 0)
setF()
test.expect(getF(), 99)

// transitive share: an inner closure captures the outer closure's
// cell through the env (LoadCapture-forwarded pointer). The forwarded
// pointer must carry the cell's array type so the construction
// retain is real — an i64-typed forward made the retain a no-op
// while the release cascade still fired, freeing the cell under the
// outer closure (index-out-of-bounds on the next call).
fn makeNested(): fn(): i64 {
    let n = 0
    fn(): i64 {
        let g = fn() { n = n + 1 }
        g()
        g()
        n
    }
}
let nested = makeNested()
test.expect(nested(), 2)
test.expect(nested(), 4)

let base = test.liveAllocBytes()
churnObject()
churnPrim()
churnNoCall()
let i = 0
while i < 200 {
    let cc = makeCounter()
    test.expect(cc(), 1)
    let nn = makeNested()
    test.expect(nn(), 2)
    i = i + 1
}
let after = test.liveAllocBytes()
test.expectTrue(after - base < 1024)
