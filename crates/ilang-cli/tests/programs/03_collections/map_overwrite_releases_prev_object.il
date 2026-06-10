// Overwriting the same key in a `Map<K, Object>` must release the
// previous occupant. `__map_get` used to retain on read while the
// lowerer classified `m[k]` as a borrow — the binding / tail-borrow
// retains stacked a second +1 nobody released, so every overwritten
// entry leaked (24 bytes/iter here). `m[k]` is now a borrowed read
// (same convention as `ArrayLoad`); this fixture pins the balance
// across overwrite, aliasing reads, delete, and map drop.

use std.test as test

class Box {
    n: i64
    init(x: i64) { this.n = x }
}

fn churnStr() {
    let m = new Map<string, Box>()
    let i = 0
    while i < 200 {
        m["a"] = new Box(i + 1)        // same key — overwrite every lap
        test.expect(m["a"].n, i + 1)   // transient read (borrow)
        let alias = m["a"]             // aliasing read (binding retains)
        test.expect(alias.n, i + 1)
        m["d"] = new Box(i)
        test.expectBool(m.delete("d"), true)
        i = i + 1
    }
    m.clear()
}

fn churnInt() {
    let m = new Map<i64, Box>()
    let i = 0
    while i < 200 {
        m[5] = new Box(i + 1)
        test.expect(m[5].n, i + 1)
        i = i + 1
    }
    // no clear — the map drop's value cascade releases the occupant
}

let base = test.liveAllocBytes()
churnStr()
churnInt()
let after = test.liveAllocBytes()
test.expectTrue(after - base < 1024)
