// `Map<K, ClassT>["k"] = newValue` must release the displaced
// previous value so `deinit` fires immediately, not at program
// exit. Pre-fix `host_map_set` did `m.inner.insert(mk, value)`
// and dropped the returned old value without calling
// `release_object` for `val_kind == 1` (Object) maps — every
// overwrite leaked the deinit until the whole map dropped.
//
// Verifies via a shared deinit counter instead of `liveAllocBytes`
// because the underlying object-cell memory free is gated on a
// separate fix; here we only check that the displaced rc reaches
// zero and triggers deinit.
//

use std.test as test
class Counter {
    n: i64
    init() { this.n = 0 }
    bump() { this.n = this.n + 1 }
    get(): i64 { this.n }
}

class Tracked {
    parent: Counter
    init(p: Counter) { this.parent = p }
    deinit() { this.parent.bump() }
}

let deinits = new Counter()
let m = new Map<i64, Tracked>()

// First insert — no displacement.
m[0] = new Tracked(deinits)
test.expect(deinits.get(), 0)

// 1000 overwrites — each must immediately drop the previous Tracked.
let i: i64 = 0
while i < 1000 {
    m[0] = new Tracked(deinits)
    i = i + 1
}

// Every overwrite displaces the prior occupant, which fires its
// deinit. The map still holds the latest one, so it doesn't
// contribute to the count yet.
test.expect(deinits.get(), 1000)
