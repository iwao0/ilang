// Parent ↔ Child where Child holds a `.weak` back-ref to Parent.
// This is the canonical use of weak refs: prevents a cycle that would
// otherwise leak (parent held by child held by parent).
use test

class DropCounter {
    n: i64
    init() { this.n = 0 }
    bump() { this.n = this.n + 1 }
    get(): i64 { this.n }
}

class Parent {
    counter: DropCounter
    init(c: DropCounter) { this.counter = c }
    deinit() { this.counter.bump() }
}

class Child {
    parent: Parent.weak
    counter: DropCounter
    init(p: Parent, c: DropCounter) { this.parent = p; this.counter = c }
    deinit() { this.counter.bump() }
}

let total = new DropCounter()

{
    let p = new Parent(total)
    let c = new Child(p, total)
    // p holds nothing about c (no strong ref), c holds weak parent.
    // No cycle in strong rcs → both should die at scope end.
}
test.expect(total.get(), 2)   // both Parent and Child got their deinit

// Weak doesn't keep parent alive.
let w_view = new DropCounter()
let weak_handle: Parent.weak = {
    let p2 = new Parent(w_view)
    let w: Parent.weak = p2
    w
}
// p2 fell out of the inner block; weak_handle now points to a freed
// Parent. .get() must return None.
let g = weak_handle.get()
test.expectBool(g.isNone(), true)
test.expect(w_view.get(), 1)   // Parent's deinit fired
