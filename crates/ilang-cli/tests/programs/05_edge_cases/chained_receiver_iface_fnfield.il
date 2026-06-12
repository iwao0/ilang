// Same fresh-intermediate-receiver leak the direct-method path had
// (round 29), now closed on the two remaining dispatch paths:
//   - interface VirtCall:  `mkHolder().grab().n`
//   - closure-field call:  `mkObj().fnField().n`
// Both guarded the receiver release behind `!matches!(ret, Object)`, so
// a chain whose intermediate returned an Object leaked that
// intermediate. Interface methods only reach ilang classes (COM goes
// through a separate path, @objc dispatches via objc_msgSend), and a
// closure-field call returns the CLOSURE's owned value — never a
// receiver alias — so releasing the fresh receiver is safe for Object
// returns in both. Static-method chains (`Factory.make().grab()`)
// already routed through the round-29 direct-method fix; kept here as a
// guard. Exact deinit + churn delta=0.

use std.test as test

let deinits: i64[] = [0]

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

interface Holder {
    grab(): Box
}
class Impl: Holder {
    b: Box
    init(x: i64) { this.b = new Box(x) }
    grab(): Box { this.b }
}
class Factory {
    static make(x: i64): Impl { new Impl(x) }
}
class Fns {
    // Non-capturing closure on purpose: a CAPTURING closure stored in a
    // class field has a separate, pre-existing leak (the closure block
    // isn't cascade-released on drop) that would mask this fixture's
    // signal. This fixture pins the receiver release, not closure ARC.
    f: fn(): Box
    init() { this.f = fn(): Box { new Box(8) } }
}

fn mkHolder(): Holder { new Impl(7) }
fn mkFns(): Fns { new Fns() }

// (A) interface chain on a fresh receiver. 1 Box/round.
fn ifaceChain(): i64 {
    mkHolder().grab().n                  // 7
}
// (B) closure-field call on a fresh receiver. 1 Box/round.
fn fnFieldChain(): i64 {
    mkFns().f().n                        // 8
}
// (C) static-method chain (round-29 path, regression guard). 1 Box/round.
fn staticChain(): i64 {
    Factory.make(9).grab().n             // 9
}

let acc: i64[] = [0]

test.expect(ifaceChain(), 7)
test.expect(fnFieldChain(), 8)
test.expect(staticChain(), 9)

// Per round: 3 functions, each makes one Box -> 3 deinits.
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    acc[0] = acc[0] + ifaceChain() + fnFieldChain() + staticChain()
    i = i + 1
}
test.expect(deinits[0] - bd, 300)
test.expect(test.liveAllocBytes() - d0, 0)
