// `init` may call methods on `this` that subclasses override. Even
// though the call appears in the BASE init body, virtual dispatch
// must resolve to the subclass override (the receiver's runtime
// class). This is a vtable-correctness check distinct from regular
// method overrides outside init.
use test

class Counter {
    n: i64
    init() {
        this.n = 0
        this.bump()
        this.bump()
        this.bump()
    }
    bump() { this.n = this.n + 1 }
}
let c = new Counter()
test.expect(c.n, 3)

// Subclass overrides a method called by base's init.
class Base {
    val: i64
    init() {
        this.val = 0
        this.setup()
    }
    setup() { this.val = 10 }
}
class Sub extends Base {
    init() { super() }
    override setup() { this.val = 100 }
}

let b = new Base()
test.expect(b.val, 10)

let s = new Sub()
test.expect(s.val, 100)        // override fires from base's init

// Sub's setup uses fields that base init has already set.
class Doubler extends Base {
    init() { super() }
    override setup() { this.val = (this.val + 1) * 2 }
}
let d = new Doubler()
test.expect(d.val, 2)          // base set val=0, then override doubles (0+1)*2
