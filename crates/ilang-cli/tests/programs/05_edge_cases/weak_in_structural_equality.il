// Clean-round pin: a `weak` reference participates in structural equality
// (rounds 127/129) by REFERENCE — two weaks to the same target are equal,
// to different targets are not. This pins weak in a tuple `==`, an enum
// with a weak payload (`==` and as a Set element / `has`), since those
// cross-feature combinations (weak × structural eq × payload-enum keys)
// were untested.
//
// expect: ok
use std.test as test

class Node {
    n: i64
    init(x: i64) { this.n = x }
}
fn target(o: Node?): i64 {
    match o {
        some(nd) { nd.n }
        none { return -1 }
    }
}

let a = new Node(1)
let b = new Node(2)
let wa: Node.weak = a
let wa2: Node.weak = a          // a second weak to the same target
let wb: Node.weak = b

// --- weak in a tuple, structural `==` (reference-compared slot) ---
let ta: (Node.weak, i64) = (wa, 5)
let ta2: (Node.weak, i64) = (wa2, 5)
let tb: (Node.weak, i64) = (wb, 5)
test.expectTrue(ta == ta2)       // same target
test.expectTrue(!(ta == tb))     // different target
test.expectTrue(ta != tb)

// --- enum with a weak payload: `==` ---
enum Ref {
    nil
    holds: (Node.weak)
}
test.expectTrue(Ref.holds(wa) == Ref.holds(wa2))   // same target
test.expectTrue(!(Ref.holds(wa) == Ref.holds(wb)))
test.expectTrue(!(Ref.holds(wa) == Ref.nil))
test.expectTrue(Ref.nil == Ref.nil)

// --- enum-weak-payload as a Set element (structural dedup) ---
let s = new Set<Ref>()
s.add(Ref.holds(wa))
s.add(Ref.holds(wa2))            // same target -> dedup
s.add(Ref.holds(wb))
s.add(Ref.nil)
s.add(Ref.nil)                   // interned unit -> dedup
test.expect(s.size(), 3)
test.expectTrue(s.has(Ref.holds(wa)))
test.expectTrue(s.has(Ref.holds(wb)))
test.expectTrue(s.has(Ref.nil))

// --- weak still upgrades after all this ---
test.expect(target(wa.get()), 1)
test.expect(target(wb.get()), 2)

console.log("ok")
