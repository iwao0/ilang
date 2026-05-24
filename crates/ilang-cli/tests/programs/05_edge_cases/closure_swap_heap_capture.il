// Mutable closure capture of a heap-typed value (a class instance),
// where the body REASSIGNS the captured slot. The cell-write path
// has to release the old value's reference and retain the new one;
// a regression that drops either side leaks the previous instance
// or double-frees the new one, both of which would manifest as a
// later use-after-free or wrong field reads.
use std.test

class Box {
    v: i64
    init(x: i64) { this.v = x }
}

fn makeSwapper(): fn(Box): Box {
    let stash: Box = new Box(0)
    fn(incoming: Box): Box {
        let prev = stash
        stash = incoming
        prev
    }
}

let s = makeSwapper()
let a = new Box(1)
let b = new Box(2)

let r1 = s(a)
test.expect(r1.v, 0)        // initial Box(0)

let r2 = s(b)
test.expect(r2.v, 1)        // a (stashed last)

let r3 = s(new Box(99))
test.expect(r3.v, 2)        // b

// `a` was stashed then replaced — but we still hold a strong via r1
// so it must be alive and readable.
test.expect(a.v, 1)
test.expect(b.v, 2)
