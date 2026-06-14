// A doubly-linked list: `next` is a strong forward chain, `prev` a weak
// back-chain. Existing weak fixtures cover parent ↔ child (tree-shaped)
// structures; this pins a LINEAR list walked in BOTH directions — forward
// over the strong `next` links and backward by upgrading each weak `prev`
// with `.get()`. The weak back-chain is what keeps the list acyclic for
// ARC: dropping the head releases the whole `next` chain and every node
// deinits (a strong `prev` would form cycles that never collect).
//
// expect: ok
use std.test as test

class Tally { n: i64; init() { this.n = 0 } }
let t = new Tally()

class Node {
    pub v: i64
    pub next: Node?
    pub prev: Node.weak?
    init(v: i64) { this.v = v; this.next = none; this.prev = none }
    deinit() { t.n = t.n + 1 }
}

fn run() {
    // 1 <-> 2 <-> 3
    let a = new Node(1)
    let b = new Node(2)
    let c = new Node(3)
    a.next = some(b); b.prev = some(a)
    b.next = some(c); c.prev = some(b)

    // Forward via the strong `next` chain.
    let fwd = 0
    let cur: Node? = some(a)
    let go = true
    while go {
        if let some(n) = cur { fwd = fwd + n.v; cur = n.next } else { go = false }
    }
    test.expect(fwd, 6)

    // Backward via the weak `prev` chain (upgraded with `.get()`).
    let back = 0
    let bcur: Node? = some(c)
    let go2 = true
    while go2 {
        if let some(n) = bcur {
            back = back + n.v
            if let some(p) = n.prev {
                if let some(pn) = p.get() { bcur = some(pn) } else { go2 = false }
            } else { go2 = false }
        } else { go2 = false }
    }
    test.expect(back, 6)
}
run()

// Every node deinits: the weak `prev` back-chain breaks the would-be
// cycle, so dropping the head releases the whole list.
test.expect(t.n, 3)
console.log("ok")
