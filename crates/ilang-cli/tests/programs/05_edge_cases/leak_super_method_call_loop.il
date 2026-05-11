// `super.inc()` inside an override resolves at compile time to the
// parent class's specific function and is dispatched directly (no
// vtable walk). The call passes `this` through unchanged — pre-fix
// a missing release on the call site's receiver clone would leak
// one strong rc per super-call, in addition to whatever the body
// itself does. Pairs with `deep_inheritance_super_chain.il`
// (functional behaviour); this fixture pins down the no-leak per
// call.
//

use test

class Base {
    n: i64
    init() { this.n = 0 }
    inc(): i64 {
        this.n = this.n + 1
        this.n
    }
}

class Sub extends Base {
    init() { super() }
    override inc(): i64 { super.inc() + 10 }
}

let baseline = test.liveAllocBytes()

let s = new Sub()
let i: i64 = 0
while i < 500 {
    let _ = s.inc()
    i = i + 1
}

let after = test.liveAllocBytes()
test.expectTrue(after - baseline < 32768)
