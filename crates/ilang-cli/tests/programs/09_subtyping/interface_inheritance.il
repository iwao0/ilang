// Plain (non-@com) interface inheritance: `interface B: A` means a B is
// also an A. A class implementing B must satisfy A's methods too, a
// B-typed receiver can call A's inherited methods, and a B (or the
// class) is assignable to an A.
//
// Before: the type checker resolved inherited interface methods through
// the parent chain, but MIR lowering only looked up the leaf interface's
// own slots, so `b.foo()` aborted with an internal "interface `B` has no
// method `foo`". MIR now reuses the ancestor's global method slot (the
// implementing class registers inherited methods there), and class→
// ancestor-interface conformance / assignability are recognised.
//
// expect: ok
use std.test as test

interface A { foo(): i64 }
interface B: A { bar(): i64 }
interface C: B { baz(): i64 }

class Impl: C {
    k: i64
    init(k: i64) { this.k = k }
    foo(): i64 { this.k }          // inherited from A (via B, C)
    bar(): i64 { this.k * 10 }     // inherited from B (via C)
    baz(): i64 { this.k * 100 }    // C's own
}

let obj = new Impl(2)

// Call through the leaf interface — own + every inherited method.
let c: C = obj
test.expect(c.foo(), 2)
test.expect(c.bar(), 20)
test.expect(c.baz(), 200)

// A mid interface sees its own + the grandparent's method.
let b: B = obj
test.expect(b.foo(), 2)
test.expect(b.bar(), 20)

// Assignable all the way up to the root interface (transitive
// conformance Impl: C, C: B, B: A).
let a: A = obj
test.expect(a.foo(), 2)

// Dynamic dispatch through an interface-typed array on an inherited
// method resolves to the concrete class's implementation.
let xs: B[] = [new Impl(3), new Impl(4)]
let total = 0
for x in xs { total = total + x.foo() }   // foo is A's, inherited by B
test.expect(total, 7)

console.log("ok")
