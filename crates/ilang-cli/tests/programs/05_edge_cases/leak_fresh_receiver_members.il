// Fresh receivers of builtin members must drop their transient +1
// after the dispatch. Three sibling paths used to leak the whole
// receiver per call:
//   - method calls on fresh string / array / Optional receivers
//     (`("v" + s).includes(..)`, `[new Box(1)].indexOf(..)`)
//   - `.length` on fresh arrays / strings (field path, not method)
//   - `.isSome` / `.isNone` on fresh Optionals
// Transfer-semantics results (`pop` / `shift` hand the slot's share
// to the caller, `find` retains into its Optional, `unwrap` retains
// the inner) must survive the receiver's release.

use std.test as test

class Box {
    n: i64
    init(x: i64) { this.n = x }
}

fn makeBoxes(): Box[] { [new Box(1), new Box(2), new Box(3)] }

// results that outlive the released fresh receiver
if let some(p) = makeBoxes().pop() { test.expect(p.n, 3) }
if let some(s) = makeBoxes().shift() { test.expect(s.n, 1) }
if let some(f) = makeBoxes().find(fn(b: Box): bool { b.n == 2 }) {
    test.expect(f.n, 2)
}
test.expect(some(new Box(5)).unwrap().n, 5)
test.expect(makeBoxes().map(fn(b: Box): i64 { b.n })[2], 3)
test.expect(makeBoxes().slice(1, 3).length, 2)

// object receivers: no leak across the member reads
let objBase = test.liveAllocBytes()
let i = 0
while i < 200 {
    test.expect(makeBoxes().length, 3)
    test.expect([new Box(7)].indexOf(new Box(7)), -1)   // identity compare
    test.expectBool(some(new Box(6)).isSome, true)
    test.expectBool(some(new Box(6)).isNone, false)
    test.expect(some(new Box(8)).unwrap().n, 8)
    i = i + 1
}
let objAfter = test.liveAllocBytes()
test.expectTrue(objAfter - objBase < 1024)

// string receivers: registry stays balanced
let strBase = test.liveStringCount()
i = 0
while i < 200 {
    test.expectTrue(("v" + `${i}`).length >= 2)
    test.expectBool(("a" + "b").includes("a"), true)
    test.expectBool(("x" + `${i}`).startsWith("x"), true)
    i = i + 1
}
let strAfter = test.liveStringCount()
test.expectTrue(strAfter - strBase < 16)
