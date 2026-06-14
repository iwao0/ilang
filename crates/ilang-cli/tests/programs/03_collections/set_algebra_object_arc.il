// `set_of_class.il` checks the result SIZES and membership of
// union / intersection / difference on object-typed sets, but not
// their ARC: the result sets share element references with the source
// sets, so each shared element must be retained while any result holds
// it and released exactly once when the last holder drops. This pins
// that — every distinct instance deinits exactly once (no leak, no
// double-free) after the sets and their algebra results all drop.
use std.test as test

class Tally { n: i64; init() { this.n = 0 } }
let tally = new Tally()

class P {
    pub x: i64
    pub init(x: i64) { this.x = x }
    pub equals(other: P): bool { this.x == other.x }
    pub hashCode(): i64 { this.x }
    deinit() { tally.n = tally.n + 1 }
}

let base = test.liveAllocBytes()
{
    let a = new Set<P>()
    a.add(new P(1))
    a.add(new P(2))
    a.add(new P(3))
    let b = new Set<P>()
    b.add(new P(2))   // value-equal to a's P(2) but a distinct instance
    b.add(new P(4))
    // 5 distinct P instances created. The algebra results alias them:
    //   union {1,2,3,4}  intersection {2}  difference {1,3}
    let u = a.union(b)
    let i = a.intersection(b)
    let d = a.difference(b)
    test.expect(u.size(), 4)
    test.expect(i.size(), 1)
    test.expect(d.size(), 2)
    let _ = u
    let _ = i
    let _ = d
}
// Every P (including the two value-equal `2`s and the shared 1/3/4)
// deinits exactly once.
test.expect(tally.n, 5)
let after = test.liveAllocBytes()
test.expectTrue(after - base < 1024)
