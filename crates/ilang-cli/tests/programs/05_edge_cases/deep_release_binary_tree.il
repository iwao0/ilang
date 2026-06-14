// Companion to `deep_release_iterative.il`, which only tears down
// LINEAR graphs (a `next`-linked list and a cons-list). Here every
// node owns TWO heap children (`left` and `right`), so releasing one
// node defers two nested releases onto the worklist instead of one.
// A balanced tree therefore exercises the branching case: the worklist
// holds many pending children at once and must drain all of them
// iteratively, with call depth staying O(1) and each node's deinit
// firing exactly once (no leak, no double-free).

use std.test as test

class Tally { n: i64; init() { this.n = 0 } }
let tally = new Tally()

class Tree {
    left: Tree?
    right: Tree?
    init() { this.left = none; this.right = none }
    deinit() { tally.n = tally.n + 1 }
}

fn build(depth: i64): Tree? {
    if depth <= 0 { return none }
    let t = new Tree()
    t.left = build(depth - 1)
    t.right = build(depth - 1)
    return some(t)
}

let base = test.liveAllocBytes()
{
    // depth 16 → 2^16 - 1 = 65535 nodes, each with two children.
    let root = build(16)
    let _ = root
}
// Every node deinits exactly once.
test.expect(tally.n, 65535)
// And nothing is left allocated — the whole tree is reclaimed.
let after = test.liveAllocBytes()
test.expectTrue(after - base < 1024)
