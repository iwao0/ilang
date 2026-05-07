// Weak references: don't bump strong rc; `.get()` returns T? (Some
// while target is alive, None after target is freed).
use test

class Node {
    label: i64
    init(v: i64) { this.label = v }
}

let strong = new Node(42)
let w: Node.weak = strong          // strong → weak auto-downgrade

// While the strong ref is alive, weak.get() returns Some.
{
    let g1 = w.get()
    test.expectBool(g1.isSome, true)
    test.expect(g1.unwrap().label, 42)
}
// `g1` is gone now; only `strong` keeps Node 42 alive.

// Drop the strong ref by re-binding to a different Node. The old
// Node has no remaining strong refs → freed.
strong = new Node(7)

// Old weak now sees None.
let g2 = w.get()
test.expectBool(g2.isNone, true)

// Get a fresh weak from the new Node and confirm it's alive.
let w2: Node.weak = strong
{
    let g3 = w2.get()
    test.expectBool(g3.isSome, true)
    test.expect(g3.unwrap().label, 7)
}
