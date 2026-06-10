// ARC for destructuring binds. Each heap binding takes its own
// retained share (the source keeps its slot shares for its own
// release cascade), and a fresh source's transient +1 drops after
// the extraction. Two pinned bugs: a fresh tuple was never released
// (the 32-byte cell leaked per `let (a, b) = makePair()`), and
// bindings didn't retain — destructuring a BORROWED tuple let the
// bindings' scope-exit releases steal the tuple's element shares,
// so the tuple's later cascade walked freed cells.

use std.test as test

class Box {
    n: i64
    init(x: i64) { this.n = x }
}

class Pair {
    a: Box
    b: string
    init(i: i64) { this.a = new Box(i); this.b = "s" + `${i}` }
}

fn makePair(i: i64): (Box, string) { (new Box(i), "s" + `${i}`) }

// fresh tuple source
let (b0, s0) = makePair(7)
test.expect(b0.n, 7)
test.expectStr(s0, "s7")

// borrowed tuple source — bindings and tuple both stay valid
fn borrowed() {
    let t = (new Box(1), new Box(2))
    let (x, y) = t
    test.expect(x.n + y.n, 3)
    test.expect(t[0].n, 1)     // tuple still owns its slots
}
borrowed()

// struct destructure: fresh and borrowed sources
let Pair { a, b } = new Pair(5)
test.expect(a.n, 5)
test.expectStr(b, "s5")

fn borrowedStruct() {
    let p = new Pair(9)
    let Pair { a, b } = p
    test.expect(a.n, 9)
    test.expect(p.a.n, 9)      // source keeps its field shares
}
borrowedStruct()

// churn balance across all four shapes
let base = test.liveAllocBytes()
let sbase = test.liveStringCount()
let i = 0
while i < 200 {
    let (fb1, fs1) = makePair(i)
    test.expect(fb1.n, i)
    test.expectTrue(fs1.length >= 2)
    let t = (new Box(1), new Box(2))
    let (x, y) = t
    test.expect(x.n + y.n, 3)
    let Pair { a, b } = new Pair(i)
    test.expect(a.n, i)
    let p = new Pair(i)
    let Pair { a, b } = p
    test.expect(a.n, i)
    i = i + 1
}
test.expectTrue(test.liveAllocBytes() - base < 1024)
test.expectTrue(test.liveStringCount() - sbase < 16)
