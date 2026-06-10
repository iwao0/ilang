// `arr.fill(v)` retains v once per slot (the array fully owns its
// copies) and releases each old occupant. Two pinned regressions:
// the lowerer never dropped a fresh fill value's transient +1
// (`arr.fill(new Box(9))` leaked the value per call), and the runtime
// released the occupant *before* retaining the incoming value — for
// the self-fill alias `arr.fill(arr[0])` that freed the value at its
// own slot before the retain could keep it.

use std.test as test

class Box {
    n: i64
    init(x: i64) { this.n = x }
}

// fresh fill value churn: old occupants and the transient both drop
let base = test.liveAllocBytes()
let i = 0
while i < 200 {
    let arr: Box[] = [new Box(1), new Box(2), new Box(3)]
    arr.fill(new Box(9))
    test.expect(arr[0].n + arr[1].n + arr[2].n, 27)
    i = i + 1
}
let after = test.liveAllocBytes()
test.expectTrue(after - base < 1024)

// self-fill: the needle aliases slot 0 — retain-before-release keeps
// it alive while its own slot is overwritten
let arr: Box[] = [new Box(7), new Box(8)]
arr.fill(arr[0])
test.expect(arr[0].n, 7)
test.expect(arr[1].n, 7)

// borrowed fill value: the source binding keeps its own share
let shared = new Box(5)
let arr2: Box[] = [new Box(1), new Box(2)]
arr2.fill(shared)
test.expect(shared.n, 5)
test.expect(arr2[0].n + arr2[1].n, 10)

// string fill keeps the registry balanced
let sbase = test.liveStringCount()
i = 0
while i < 200 {
    let sa: string[] = ["a", "b"]
    sa.fill("x" + `${i}`)
    test.expectStr(sa[1], `x${i}`)
    i = i + 1
}
let safter = test.liveStringCount()
test.expectTrue(safter - sbase < 16)
