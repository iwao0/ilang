// ARC balance for heap-valued properties. Property reads are OWNED:
// the getter's tail retains like any method tail (computed getters
// whose tail is fresh own their +1 anyway), and the consumer side
// classifies `obj.prop` as fresh and drops the share. Setter calls
// release a fresh value's transient +1 after the dispatch (the
// setter body's store takes its own retain). Three pinned leaks:
// `h.prop = new Box(..)` leaked the value per call, transient
// `h.prop.n` reads leaked the getter's +1 once the occupant was
// overwritten, and `get fresh(): Box { new Box(..) }` leaked one
// object per read.

use std.test as test

class Box {
    n: i64
    init(x: i64) { this.n = x }
}

class Holder {
    inner: Box
    init() { this.inner = new Box(0) }
    get item(): Box { this.inner }
    set item(v: Box) { this.inner = v }
    // explicit `this.` tail (the bare-var tail shape is pinned by
    // getter_tail_bare_var_heap.il)
    get same(): Box { this.inner }
}

class Factory {
    seed: i64
    init() { this.seed = 5 }
    // computed property minting a fresh object per read
    get fresh(): Box { new Box(this.seed) }
}

let h = new Holder()
let f = new Factory()

// alias semantics across overwrite
h.item = new Box(1)
let alias = h.item
h.item = new Box(2)
test.expect(alias.n, 1)
test.expect(h.item.n, 2)
test.expect(h.same.n, 2)

// borrowed setter value: the source binding keeps its own share
let shared = new Box(9)
h.item = shared
test.expect(shared.n, 9)
h.item = new Box(3)
test.expect(shared.n, 9)

// churn: setter fresh values, transient reads, alias binds, and
// fresh-tail computed reads all balance
let base = test.liveAllocBytes()
let i = 0
while i < 200 {
    h.item = new Box(i)
    test.expect(h.item.n, i)
    let kept = h.item
    h.item = new Box(i + 1)
    test.expect(kept.n, i)
    test.expect(f.fresh.n, 5)
    let minted = f.fresh
    test.expect(minted.n, 5)
    i = i + 1
}
let after = test.liveAllocBytes()
test.expectTrue(after - base < 1024)
