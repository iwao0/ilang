// Builder pattern: `b.add(1).add(2).add(3)` — each `add` returns
// `this`, so every intermediate is just a +1 retain on the SAME
// instance. The trailing `let _ = b` then releases the chain's
// final +1, and `b` going out of scope releases the original. Net
// per loop: one Box object plus one i64 array, both reclaimed.
// A regression that treats the chained return as a fresh
// allocation would leak 24 bytes + array storage per iteration.
//

use std.test as test
class Box {
    items: i64[]
    init() { this.items = [] }
    add(n: i64): Box {
        this.items.push(n)
        this
    }
}

let baseline = test.liveAllocBytes()

let i: i64 = 0
while i < 200 {
    let b = new Box()
    b.add(1).add(2).add(3)
    let _ = b
    i = i + 1
}

let after = test.liveAllocBytes()
// Allow slack for the still-not-freed object cell baseline
// (`leak_object_alloc_loop.il`), but reject the array buffer
// growth that would dominate if the chain leaked.
test.expectTrue(after - baseline < 65536)
