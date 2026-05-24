// `next: Self?` field forms a linked list. Walking it via `if let
// some(n) = node.next` recurses cleanly; reassigning `next` to `none`
// detaches the rest of the chain (the original tail's nodes drop iff
// no other binding holds them). Tests both walk semantics and the
// retain/release of the Optional<Object> field at reassignment time.
use std.test

class Linked {
    v: i64
    next: Linked?
    init(x: i64) {
        this.v = x
        this.next = none
    }
}

let head = new Linked(1)
let mid = new Linked(2)
let tail = new Linked(3)
head.next = some(mid)
mid.next = some(tail)

fn sum(n: Linked?): i64 {
    if let some(node) = n {
        node.v + sum(node.next)
    } else {
        0
    }
}
test.expect(sum(some(head)), 6)

fn length(n: Linked?): i64 {
    if let some(node) = n {
        1 + length(node.next)
    } else {
        0
    }
}
test.expect(length(some(head)), 3)
test.expect(length(none), 0)

// Detach from `head` — chain shrinks.
head.next = none
test.expect(sum(some(head)), 1)
test.expect(length(some(head)), 1)

// Re-attach `tail` directly (skipping mid).
head.next = some(tail)
test.expect(sum(some(head)), 4)         // 1 + 3
test.expect(length(some(head)), 2)

// `mid` is still alive because the local binding holds it.
test.expect(mid.v, 2)
