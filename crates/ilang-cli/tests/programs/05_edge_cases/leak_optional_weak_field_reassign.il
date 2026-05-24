// `obj.weak_field = some(other)` — an Optional<Weak<T>> field.
// The weak rc on the target is bumped on `some(...)` and
// released when the Optional shell drops at the next iteration's
// reassignment. Pre-fix the field-store path could miss the
// weak-release on the displaced Optional, leaving stale weak
// counts that prevent eventual deallocation.
//

use std.test

class Node {
    parent: Node.weak?
    init() { this.parent = none }
}

let baseline = test.liveAllocBytes()

let p1 = new Node()
let p2 = new Node()
let c = new Node()
let i: i64 = 0
while i < 200 {
    c.parent = if i % 2 == 0 { some(p1) } else { some(p2) }
    let _ = c.parent.isSome
    i = i + 1
}

let after = test.liveAllocBytes()
test.expectTrue(after - baseline < 65536)
