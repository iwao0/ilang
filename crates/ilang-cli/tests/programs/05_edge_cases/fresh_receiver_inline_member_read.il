// Reading a member straight off a fresh-receiver index —
// `makeMap()["k"].n` / `makeArr()[1].n` — lowers to
// `retain v; release receiver; load_field v; release v`.
// arc_peephole used to treat `Release <other>` as safe-to-cross and
// cancelled the retain/release pair around the receiver's release,
// so the receiver's cascade freed the value before the load and the
// field read returned garbage. Releases of other values are barriers
// now (a container release can cascade into the candidate's pointee).

use std.test as test

class Box {
    n: i64
    init(x: i64) { this.n = x }
}

fn makeMap(): Map<string, Box> {
    let t = new Map<string, Box>()
    t["k"] = new Box(42)
    t
}

fn makeArr(): Box[] {
    [new Box(41), new Box(42)]
}

let base = test.liveAllocBytes()
let i = 0
while i < 200 {
    test.expect(makeMap()["k"].n, 42)
    test.expect(makeArr()[1].n, 42)
    i = i + 1
}
let after = test.liveAllocBytes()
test.expectTrue(after - base < 1024)

// bound forms read the same values (the pre-fix behaviour differed
// between the two — binding worked, inline read garbage)
let viaMap = makeMap()["k"]
test.expect(viaMap.n, 42)
let viaArr = makeArr()[1]
test.expect(viaArr.n, 42)
