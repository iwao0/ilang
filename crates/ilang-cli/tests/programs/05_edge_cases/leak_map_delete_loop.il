// `Map.delete(k)` must release the removed value so `deinit`
// fires at the deletion site, not at program exit. Pre-fix
// `host_map_delete` discarded the removed value without
// `release_object`, so per-iteration insert+delete cycles
// silently leaked deinits.
//
// Verifies via a shared deinit counter instead of `liveAllocBytes`
// because the underlying object-cell memory free is gated on a
// separate fix.
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

let i: i64 = 0
while i < 1000 {
    m[i] = new Tracked(deinits)
    m.delete(i)
    i = i + 1
}

// Every delete should fire the value's deinit immediately.
test.expect(deinits.get(), 1000)
