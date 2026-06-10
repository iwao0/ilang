// Releasing a deep object graph must not recurse per link: the
// per-kind cascade used to call itself through every
// object → optional → object hop and blew the native stack at
// ~100k links. `release_field_by_kind` now defers nested releases
// to a worklist drained iteratively by the outermost frame, so call
// depth stays O(1) regardless of graph depth. Deinits still fire
// exactly once per node.

use std.test as test

class Node {
    next: Node?
    payload: i64[]
    init() { this.next = none; this.payload = [1, 2] }
}

fn buildList(n: i64): Node {
    let head = new Node()
    let cur = head
    let i = 0
    while i < n {
        let nx = new Node()
        cur.next = some(nx)
        cur = nx
        i = i + 1
    }
    head
}

enum Chain {
    nil
    cons: (i64, Chain)
}

fn buildChain(n: i64): Chain {
    let c = Chain.nil
    let i = 0
    while i < n {
        c = Chain.cons(i, c)
        i = i + 1
    }
    c
}

class Tally { n: i64; init() { this.n = 0 } }
let tally = new Tally()

class Counted {
    next: Counted?
    init() { this.next = none }
    deinit() { tally.n = tally.n + 1 }
}

fn buildCounted(n: i64): Counted {
    let head = new Counted()
    let cur = head
    let i = 0
    while i < n {
        let nx = new Counted()
        cur.next = some(nx)
        cur = nx
        i = i + 1
    }
    head
}

let base = test.liveAllocBytes()
{
    let l = buildList(100000)
    let _ = l
}
{
    let c = buildChain(100000)
    let _ = c
}
{
    let k = buildCounted(50000)
    let _ = k
}
test.expect(tally.n, 50001)
let after = test.liveAllocBytes()
test.expectTrue(after - base < 1024)
