// Calling an interface method through a PARENT-CLASS-typed receiver that
// holds a subclass instance must dispatch correctly (it used to segfault).
//
// Before: a class that implements an interface registers each interface
// method under TWO vtable slots — its class slot (e.g. 0) and the
// interface's high slot (1<<20). A subclass built its inherited slot map
// keyed by name, so the high interface slot clobbered the class slot and
// the subclass lost its class-slot entry entirely. A `Base`-typed call
// (`let b: Base = new Derived(); b.foo()`) then dispatched through an
// empty class slot and read a garbage function pointer -> SIGSEGV. A
// non-interface method, or an interface-TYPED receiver, happened to work
// (different slot), which masked it. The subclass now inherits BOTH the
// class slot and the interface slot.
//
// expect: ok
use std.test as test
let deinits: i64[] = [0]
class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

interface A { foo(): i64 }
interface B: A { bar(): i64 }     // interface inheritance, for the chained case

class Base: B {
    foo(): i64 { 1 }
    bar(): i64 { 2 }
}
class Derived: Base {
    override foo(): i64 { 10 }
    override bar(): i64 { 20 }
}
class GrandChild: Derived {
    override foo(): i64 { 100 }
}

// (1) the crashing shape: parent-class-typed receiver, interface method,
// no override in the subclass body
class PlainBase: A { foo(): i64 { 7 } }
class PlainSub: PlainBase {}        // inherits foo, declares nothing
let pb: PlainBase = new PlainSub()
test.expect(pb.foo(), 7)            // class-slot dispatch must not segfault

// (2) override reached through a parent-class-typed receiver
let b: Base = new Derived()
test.expect(b.foo(), 10)            // A.foo override via Base-typed receiver
test.expect(b.bar(), 20)            // B.bar override via Base-typed receiver

// (3) multi-level: GrandChild's override through every receiver static type
let g: Base = new GrandChild()
test.expect(g.foo(), 100)
let ga: A = new GrandChild()        // interface-typed receiver still works
test.expect(ga.foo(), 100)

// (4) a NEW (non-interface) method on the subclass — slot numbering for
// fresh methods must stay in the class range, not collide
let d = new Derived()
test.expect(d.foo(), 10)

// (5) mixed concrete subclasses behind a parent-class array, inherited
// interface method, dynamic dispatch
let xs: Base[] = [new Base(), new Derived(), new GrandChild()]
let total = 0
for x in xs { total = total + x.foo() }
test.expect(total, 111)             // 1 + 10 + 100

// (6) ARC: a heap-returning interface method dispatched through a
// parent-class receiver leaks nothing and never over-releases
interface Maker { make(): Box }
class MBase: Maker { make(): Box { new Box(1) } }
class MSub: MBase { override make(): Box { new Box(2) } }
fn churnOne(): i64 {
    let m: MBase = new MSub()
    m.make().n
}
let acc: i64[] = [0]
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 { acc[0] = acc[0] + churnOne(); i = i + 1 }
test.expect(deinits[0] - bd, 100)   // one Box made and freed per call
test.expect(test.liveAllocBytes() - d0, 0)

console.log("ok")
