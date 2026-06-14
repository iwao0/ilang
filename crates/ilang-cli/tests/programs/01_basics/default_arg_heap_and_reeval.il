// Default arguments have two subtle behaviours the existing default-arg
// fixtures don't pin: a HEAP default value is freshly built per call and
// must be released (no leak) when the parameter drops, and the default
// expression is RE-EVALUATED at every call (an omitted call re-runs it;
// an explicit arg skips it).
use std.test as test

class Tally { n: i64; init() { this.n = 0 } }
let boxes = new Tally()

class Box {
    pub v: i64
    init(v: i64) { this.v = v }
    deinit() { boxes.n = boxes.n + 1 }
}

fn unwrap(b: Box = new Box(99)): i64 { b.v }

class Counter { pub n: i64; init() { this.n = 0 } }
let c = new Counter()
fn nextId(): i64 { c.n = c.n + 1; c.n }
fn idOf(id: i64 = nextId()): i64 { id }

// Heap default: omitted builds Box(99); explicit passes Box(5).
test.expect(unwrap(), 99)
test.expect(unwrap(new Box(5)), 5)

// Re-evaluation: each omitted call re-runs nextId(); an explicit arg
// does not advance the counter.
test.expect(idOf(), 1)
test.expect(idOf(), 2)
test.expect(idOf(50), 50)
test.expect(idOf(), 3)
test.expect(c.n, 3)

// Both Boxes deinit exactly once — the per-call default did not leak.
test.expect(boxes.n, 2)
