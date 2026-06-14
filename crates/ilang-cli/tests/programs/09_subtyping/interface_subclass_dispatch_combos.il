// Clean-round pin for the interface vtable-slot family (rounds 119/120):
// a subclass of an interface-implementing class dispatches correctly
// across combinations the round-120 fixture does not cover. Each of
// these used to be a segfault candidate before the subclass kept both
// its class slot and its interface slot(s).
//
// Covers: a class implementing TWO interfaces; the SAME method name
// declared in two interfaces; `super.method()` from an interface-backed
// override reached through a parent-class receiver; a subclass that adds
// a brand-new interface its parent never implemented; a grandchild that
// inherits (does not re-declare) a parent's override; and a partial
// override (one interface method overridden, a sibling inherited). All
// are exercised through both parent-class-typed and interface-typed
// receivers.
//
// expect: ok
use std.test as test

// --- two separate interfaces on one class, with a subclass override ---
interface A1 { foo(): i64 }
interface B1 { bar(): i64 }
class Base1: A1, B1 {
    foo(): i64 { 1 }
    bar(): i64 { 2 }
}
class Derived1: Base1 {
    override foo(): i64 { 10 }
}
let b1: Base1 = new Derived1()
test.expect(b1.foo(), 10)
test.expect(b1.bar(), 2)
let a1: A1 = new Derived1()
let bb1: B1 = new Derived1()
test.expect(a1.foo(), 10)
test.expect(bb1.bar(), 2)

// --- same method name in two interfaces ---
interface A2 { ping(): i64 }
interface B2 { ping(): i64 }
class Base2: A2, B2 {
    ping(): i64 { 5 }
}
class Derived2: Base2 {
    override ping(): i64 { 50 }
}
let b2: Base2 = new Derived2()
test.expect(b2.ping(), 50)
let a2: A2 = new Derived2()
let bb2: B2 = new Derived2()
test.expect(a2.ping(), 50)
test.expect(bb2.ping(), 50)

// --- super.method() from an interface-backed override, parent receiver ---
interface A3 { foo(): i64 }
class Base3: A3 { foo(): i64 { 1 } }
class Derived3: Base3 {
    override foo(): i64 { super.foo() + 100 }
}
let b3: Base3 = new Derived3()
test.expect(b3.foo(), 101)

// --- subclass declares a NEW interface the parent never implemented ---
interface A4 { foo(): i64 }
interface X4 { qux(): i64 }
class Base4: A4 { foo(): i64 { 1 } }
class Derived4: Base4, X4 {
    override foo(): i64 { 10 }
    qux(): i64 { 99 }
}
let b4: Base4 = new Derived4()
test.expect(b4.foo(), 10)
let x4: X4 = new Derived4()
test.expect(x4.qux(), 99)

// --- grandchild inherits (does not re-declare) a parent's override ---
interface A5 { foo(): i64 }
class Base5: A5 { foo(): i64 { 1 } }
class Mid5: Base5 { override foo(): i64 { 10 } }
class Leaf5: Mid5 {}
let b5: Base5 = new Leaf5()
let m5: Mid5 = new Leaf5()
let a5: A5 = new Leaf5()
test.expect(b5.foo(), 10)
test.expect(m5.foo(), 10)
test.expect(a5.foo(), 10)

// --- partial override: foo overridden, bar inherited ---
interface A6 {
    foo(): i64
    bar(): i64
}
class Base6: A6 {
    foo(): i64 { 1 }
    bar(): i64 { 2 }
}
class Derived6: Base6 {
    override foo(): i64 { 10 }
}
let b6: Base6 = new Derived6()
test.expect(b6.foo(), 10)
test.expect(b6.bar(), 2)
let a6: A6 = new Derived6()
test.expect(a6.foo(), 10)
test.expect(a6.bar(), 2)

console.log("ok")
