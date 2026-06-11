// ARC at a `return` out of a for-in body, both directions:
//
// - the FRESH iterable (`for e in m.entries()`) is released on the
//   return path — its Release lives in the loop's exit block, which
//   `break` reaches but `return` bypassed (leaked the whole entries
//   array per call).
// - the element binding is a BORROW into the array (ArrayLoad takes
//   no retain), so the return sweep must NOT release it: as a plain
//   Ssa binding the sweep over-released the element — a long-lived
//   array's objects died while the array still pointed at them
//   (use-after-free / SIGABRT). It's now a PatternBinding, the same
//   contract as match payloads. The deinit count pins exactly one
//   release per element, and the elements stay readable after many
//   early-return scans.
//
// expect: scans ok deinits=0
// expect: fresh-iter delta-ok
// expect: still-alive 1,2,3
// expect: final deinits=3

use std.test as test

let deinits: i64[] = [0]
class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

// 非 fresh 配列 + 途中 return: 要素を過剰解放しないこと
fn findFirstAbove(arr: Box[], limit: i64): i64 {
    for b in arr {
        if b.n > limit { return b.n }
    }
    return -1
}
let data: Box[] = [new Box(1), new Box(2), new Box(3)]
let i = 0
let acc: i64[] = [0]
while i < 200 {
    acc[0] = acc[0] + findFirstAbove(data, 1)
    i = i + 1
}
test.expectTrue(acc[0] > 0)
console.log("scans ok deinits=" + deinits[0].toString())

// fresh iterable + 途中 return: iterable が回収されること
class Pair { k: string; v: i64; init(k: string, v: i64) { this.k = k; this.v = v } }
fn makePairs(n: i64): Pair[] {
    let out: Pair[] = []
    out.push(new Pair("a", n))
    out.push(new Pair("b", n + 1))
    out
}
fn scanFresh(n: i64): i64 {
    for p in makePairs(n) {
        if p.v >= n { return p.v }
    }
    return -1
}
let b1 = test.liveAllocBytes()
let j = 0
while j < 200 { acc[0] = acc[0] + scanFresh(j); j = j + 1 }
let d1 = test.liveAllocBytes() - b1
test.expectTrue(d1 < 256)
console.log("fresh-iter delta-ok")

// 過剰解放されていない証明: 要素はまだ生きて読める
console.log("still-alive " + data[0].n.toString() + "," + data[1].n.toString() + "," + data[2].n.toString())
test.expect(deinits[0], 0)

// 配列を捨てたら deinit がちょうど 3 回
let sink: Box[][] = [data]
let _ = sink.pop()
data = []
console.log("final deinits=" + deinits[0].toString())
