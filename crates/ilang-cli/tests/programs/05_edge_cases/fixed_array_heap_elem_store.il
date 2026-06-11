// Fixed-length arrays (`T[N]`) with HEAP elements — the parts that
// already work, pinned so the eventual ownership-model fix doesn't
// regress them (see HANDOFF round 18 for the parts that DON'T:
// scope-exit / early-exit / field-drop / field-overwrite element
// releases are not modeled — codegen skips rc bookkeeping for
// fixed arrays entirely):
// - `arr[i] = x` releases the overwritten element exactly once and
//   retains the incoming one (deinit fires per overwrite)
// - reading an element into a binding and returning it outlives
//   the (per-call) array storage
//
// expect: overwrite deinits=50 values-ok
// expect: escape n=8

use std.test as test
let deinits: i64[] = [0]
class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

fn overwrite(k: i64): i64 {
    let arr: Box[2] = [new Box(k), new Box(k + 1)]
    arr[0] = new Box(k + 100)
    arr[0].n + arr[1].n
}
let before = deinits[0]
let i = 0
let acc: i64[] = [0]
while i < 50 { acc[0] = acc[0] + overwrite(i); i = i + 1 }
let dd = deinits[0] - before
test.expectTrue(acc[0] > 0)
console.log("overwrite deinits=" + dd.toString() + " values-ok")

fn escape(k: i64): Box {
    let arr: Box[2] = [new Box(k), new Box(k + 1)]
    let keep = arr[1]
    keep
}
let g = escape(7)
console.log("escape n=" + g.n.toString())
