// A child class inherits its parent's `weak` field and keeps the
// weak semantics — accessing `.get()` through the inherited slot
// still produces a live `T?` while the strong owner is alive, and
// `none` once the target is freed. Per syntax.md §7 + §12.
use std.test as test

class Node {
    label: i64
    init(v: i64) { this.label = v }
}

class Holder {
    target: Node.weak
    init(n: Node) { this.target = n }
}

class TaggedHolder: Holder {
    tag: string
    init(n: Node, t: string) {
        super(n)
        this.tag = t
    }
}

// Strong owner kept alive; the child's inherited weak slot resolves.
let n = new Node(7)
let h = new TaggedHolder(n, "primary")
test.expectStr(h.tag, "primary")
{
    let g = h.target.get()
    test.expectBool(g.isSome, true)
    test.expect(g.unwrap().label, 7)
}

// Drop the last strong reference. `n` re-bind kills the original
// Node — the inherited weak now reports `none`.
n = new Node(99)
let g2 = h.target.get()
test.expectBool(g2.isNone, true)

// Point at the new Node via a freshly-constructed child holder.
let h2 = new TaggedHolder(n, "secondary")
let g3 = h2.target.get()
test.expectBool(g3.isSome, true)
test.expect(g3.unwrap().label, 99)
