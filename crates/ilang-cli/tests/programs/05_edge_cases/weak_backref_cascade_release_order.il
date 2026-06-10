// Parent → strong children array, child → `Parent.weak` back-ref.
// When the parent's release cascade frees a child, the child's field
// release weak-releases the parent — whose strong rc is already 0
// mid-cascade. `__release_weak` used to see strong == 0, decide it
// owned the free, and pull the parent's box out from under its own
// in-progress field walk; the tail of `__release_object` then freed
// it a second time (malloc abort / SIGSEGV at scope exit).
// `__release_object` now pins the box with a guard weak ref across
// the deinit + cascade and drops it last.

use std.test as test

class Parent {
    children: Child[]
    name: string
    init(n: string) {
        this.children = []
        this.name = n
    }
    add(c: Child) {
        this.children.push(c)
        c.parent = this
    }
}

class Child {
    parent: Parent.weak
    label: string
    init(l: string) { this.label = l }
}

fn cycle(n: i64) {
    let p = new Parent("root")
    let c1 = new Child("c1")
    let c2 = new Child("c2")
    p.add(c1)
    p.add(c2)
    test.expect(p.children.length, 2)
    if let some(up) = c1.parent.get() { test.expectStr(up.name, "root") }
    let _ = n
    // scope exit: p / c1 / c2 release in some order; whichever side
    // goes first, the cascade + weak back-ref must not double-free
}

// child-only survivor: the array cascade is the child's last strong
// owner, so the weak back-ref release happens inside the parent's
// own field walk
fn parentFirst() {
    let p = new Parent("root")
    p.add(new Child("only"))
}

let base = test.liveAllocBytes()
let i = 0
while i < 200 {
    cycle(i)
    parentFirst()
    i = i + 1
}
let after = test.liveAllocBytes()
test.expectTrue(after - base < 1024)

// upgrade after the strong owner is gone still answers none
fn orphan(): Child {
    let p = new Parent("gone")
    let c = new Child("waif")
    p.add(c)
    c
}
let w = orphan()
test.expectBool(w.parent.get().isNone, true)
