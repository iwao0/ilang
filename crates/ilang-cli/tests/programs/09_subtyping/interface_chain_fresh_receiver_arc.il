// Clean-round pin for the interface-dispatch ARC family (rounds 119/120):
// a FRESH interface-typed receiver whose method returns a heap value, in
// a chain (`getFactory().make().n`). The intermediate interface receiver
// is owned by nothing else, and the heap return is an owned +1 — both
// must be released. Round 119 pinned fresh interface-dispatch ARGS; this
// pins the fresh-receiver / heap-return side of the same path.
//
// Also pins nested-Optional-of-heap ARC through interface-returning
// helpers, a placement not otherwise covered for interface dispatch.
//
// expect: ok
use std.test as test
let deinits: i64[] = [0]
class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

interface Factory { make(): Box }
class FImpl: Factory {
    make(): Box { new Box(7) }
}
fn getFactory(): Factory { new FImpl() }

// Fresh interface receiver -> heap-returning method -> field read. The
// intermediate FImpl and the returned Box must both be released.
fn chainTarget(): i64 {
    getFactory().make().n
}

let acc: i64[] = [0]
let bd = deinits[0]
let d0 = test.liveAllocBytes()
test.expect(chainTarget(), 7)
test.expect(deinits[0] - bd, 1)              // the returned Box dies once
test.expect(test.liveAllocBytes() - d0, 0)

// churn: zero leak, exactly one Box per call
let bd1 = deinits[0]
let d1 = test.liveAllocBytes()
let i = 0
while i < 100 { acc[0] = acc[0] + chainTarget(); i = i + 1 }
test.expect(deinits[0] - bd1, 100)
test.expect(test.liveAllocBytes() - d1, 0)

// --- nested Optional of heap through Optional-returning helpers ---
fn unwrap1(o: Box?): i64 {
    match o {
        some(b) { b.n }
        none { return 0 }
    }
}
fn unwrap2(o: Box??): i64 {
    match o {
        some(inner) { unwrap1(inner) }
        none { return 0 }
    }
}
fn optTarget(): i64 {
    let o: Box? = new Box(3)                  // T -> T? wrap
    let o2: Box?? = some(some(new Box(4)))    // nested wrap
    unwrap1(o) + unwrap2(o2)
}
let bd2 = deinits[0]
let d2 = test.liveAllocBytes()
test.expect(optTarget(), 7)
test.expect(deinits[0] - bd2, 2)             // both boxes die
test.expect(test.liveAllocBytes() - d2, 0)

let bd3 = deinits[0]
let d3 = test.liveAllocBytes()
let j = 0
while j < 100 { acc[0] = acc[0] + optTarget(); j = j + 1 }
test.expect(deinits[0] - bd3, 200)
test.expect(test.liveAllocBytes() - d3, 0)

console.log("ok")
