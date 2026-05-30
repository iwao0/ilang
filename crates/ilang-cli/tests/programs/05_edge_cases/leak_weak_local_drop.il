// `ClassName.weak` references used to permanently tombstone the
// underlying object box: `jit_setup`/`aot` skipped registering the
// class size table for any weak-referenced class, so
// `__release_object` could never call `__mir_free`. With a runtime
// weak-counter side-table (`WEAK_COUNT_TABLE`), `__release_object`
// now defers the free when weak refs are still pending and the final
// `__release_weak` performs the free. This exercises both the local-
// weak path and the weak-field-in-class path; both must leak zero
// allocations once their containing scope drops.
use std.test as test

class Node {
    label: i64
    init(v: i64) { this.label = v }
}

class Holder {
    w: Node.weak
    init(n: Node) { this.w = n }
}

// 1) Local weak ref drops cleanly after its strong observer dies.
let base = test.liveAllocCount()
let i = 0
while i < 200 {
    let n = new Node(i)
    let w: Node.weak = n
    i = i + 1
}
test.expectTrue(test.liveAllocCount() - base < 8)

// 2) Weak ref stored as a class field releases via the field cascade.
let base2 = test.liveAllocCount()
let j = 0
while j < 200 {
    let n = new Node(j)
    let h = new Holder(n)
    j = j + 1
}
test.expectTrue(test.liveAllocCount() - base2 < 8)

// 3) A weak observer outliving its strong (rebound) is balanced
// when both finally go out of scope.
let base3 = test.liveAllocCount()
let k = 0
while k < 100 {
    let s = new Node(k)
    let w: Node.weak = s
    s = new Node(k + 1000)   // releases original Node(k); w now dangles weakly
    k = k + 1
}
test.expectTrue(test.liveAllocCount() - base3 < 8)
