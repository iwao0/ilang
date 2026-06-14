// Static methods and static properties were reachable by a subclass's
// own name (`Derived.make()`, `Derived.version`), but a static FIELD was
// not — `Derived.count` was rejected ("undefined variable Derived" in the
// checker, "unbound variable" in MIR lowering). A static field is a
// single shared slot, so the subclass name should alias the parent's.
// The checker now inherits the field's type and the MIR now inherits the
// slot id, so a subclass (and grandchild) read and write the SAME slot.
//
// expect: ok
use std.test as test

class Base {
    static count: i64 = 0
    const MAX: i64 = 100
    init() {}
}
class Derived: Base { init() { super() } }
class GrandChild: Derived { init() { super() } }

// Read an inherited static field by the subclass / grandchild name.
test.expect(Derived.count, 0)
test.expect(GrandChild.count, 0)

// Inherited const is readable too.
test.expect(Derived.MAX, 100)
test.expect(GrandChild.MAX, 100)

// One shared slot: write through any name, read through any other.
Base.count = 5
test.expect(Derived.count, 5)
Derived.count = 9
test.expect(Base.count, 9)
test.expect(GrandChild.count, 9)

console.log("ok")
