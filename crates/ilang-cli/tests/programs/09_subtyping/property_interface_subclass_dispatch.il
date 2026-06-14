// Clean-round pin: a class with virtual PROPERTIES that also implements
// an interface, plus a subclass overriding a getter, all reached through
// a parent-class-typed receiver. Property accessors are synthetic
// MethodDecls that share the same vtable-slot loop the round-120 fix
// reworked (it changed how a subclass inherits class vs interface
// slots), so this guards that property dispatch still resolves and that
// a heap getter/setter is ARC-correct in this combination.
//
// Round 103/104 pinned property virtual dispatch but without an
// interface on the class; round 120 pinned interface + subclass dispatch
// but without properties. This is their intersection.
//
// expect: ok
use std.test as test
let deinits: i64[] = [0]
class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

interface HasTag { tag(): i64 }

class Base: HasTag {
    _b: Box
    init(x: i64) { this._b = new Box(x) }
    tag(): i64 { 1 }                       // interface method
    get box(): Box { this._b }             // getter returns heap (borrow)
    set box(v: Box) { this._b = v }        // setter takes heap
    get scalar(): i64 { this._b.n }
}
class Derived: Base {
    init(x: i64) { super(x) }
    override tag(): i64 { 9 }              // override interface method
    get box(): Box { this._b }             // override getter (no `override` kw)
    get scalar(): i64 { this._b.n + 1000 } // override scalar getter
}

// interface method + overridden getters reached through a Base-typed ref
let b: Base = new Derived(5)
test.expect(b.tag(), 9)                    // overridden interface method
test.expect(b.scalar, 1005)               // overridden scalar getter
test.expect(b.box.n, 5)                    // overridden heap getter (borrow)

// inherited setter through the base ref, with a fresh heap arg; the old
// box is replaced (deinit) and the fresh one is owned by the field
let bd0 = deinits[0]
b.box = new Box(20)
test.expect(deinits[0] - bd0, 1)           // old Box(5) released, no leak of the new one
test.expect(b.scalar, 1020)               // 20 + 1000

// ARC: heap getter (borrow) + setter (fresh arg) under churn, no leak,
// no over-release
fn churnOne(): i64 {
    let d: Base = new Derived(5)
    let got = d.box.n                      // overridden getter -> heap borrow
    d.box = new Box(20)                    // inherited setter, fresh arg, replaces Box(5)
    got + d.box.n                          // 5 + 20
}
let acc: i64[] = [0]
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 { acc[0] = acc[0] + churnOne(); i = i + 1 }
test.expect(deinits[0] - bd, 200)          // Box(5) + Box(20) per call
test.expect(test.liveAllocBytes() - d0, 0)

console.log("ok")
