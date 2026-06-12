// A chained method call `a.m1().m2()` leaked the intermediate `a.m1()`
// whenever m1 returned an Object. The fresh-receiver release in object
// method lowering only fired when the method's return type was NOT an
// Object — the conservative guard was meant to avoid freeing a returned
// field-alias, but an ilang method's heap return is always OWNED (+1
// via the tail-borrow / bare-`this` retain), so the returned object
// keeps its own share and survives the receiver's drop cascade. The
// guard therefore leaked every `obj.getThing().use()` and every builder
// `mk().with(..).with(..)` chain, plus nested generics whose read path
// is `cc.get().get()` (the leak that surfaced this).
//
// Fix: release the fresh receiver for Object returns too, EXCEPT when
// the receiver is `@objc` (its Object return is an autoreleased borrow,
// not an owned +1) — pinned indirectly by the cocoa_* fixtures.
//
// A named intermediate (`let m = a.m1(); m.m2()`) never leaked — the
// binding's scope-exit release covers it — so this only bites the
// inline chain. Exact deinit + 100-round churn delta=0.

use std.test as test

let deinits: i64[] = [0]

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

// Field-returning chain: Outer.get() -> Inner, Inner.get() -> Box.
class Inner {
    b: Box
    init(x: i64) { this.b = new Box(x) }
    get(): Box { this.b }
}
class Outer {
    i: Inner
    init() { this.i = new Inner(7) }
    get(): Inner { this.i }
}

// Builder pattern: bump() returns `this`.
class Builder {
    v: i64
    box: Box
    init() { this.v = 0; this.box = new Box(1) }
    bump(): Builder { this.v = this.v + 1; this }
    value(): i64 { this.v }
}

// Generic cell — read path is the `cc.get().get()` chain.
class Cell<T> {
    val: T
    init(v: T) { this.val = v }
    get(): T { this.val }
}

fn mkOuter(): Outer { new Outer() }
fn mkBuilder(): Builder { new Builder() }

// (A) field-returning chain on a fresh receiver. 1 Box/round.
fn chainField(): i64 {
    new Outer().get().get().n                 // 7
}
// (B) same chain, receiver itself a fresh fn result. 1 Box/round.
fn chainFromCall(): i64 {
    mkOuter().get().get().n                    // 7
}
// (C) builder chain returning `this`. 1 Box/round.
fn chainBuilder(): i64 {
    mkBuilder().bump().bump().bump().value()   // 3
}
// (D) nested generic read chain. 1 Box/round.
fn chainGeneric(): i64 {
    new Cell<Cell<Box>>(new Cell<Box>(new Box(5))).get().get().n   // 5
}
// (E) named intermediate (regression guard — never leaked). 1 Box/round.
fn viaLocal(): i64 {
    let o = new Outer()
    let m = o.get()
    m.get().n                                  // 7
}

let acc: i64[] = [0]

test.expect(chainField(), 7)
test.expect(chainFromCall(), 7)
test.expect(chainBuilder(), 3)
test.expect(chainGeneric(), 5)
test.expect(viaLocal(), 7)

// Per round: 5 functions, each makes exactly one Box -> 5 deinits.
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    acc[0] = acc[0]
        + chainField() + chainFromCall() + chainBuilder()
        + chainGeneric() + viaLocal()
    i = i + 1
}
test.expect(deinits[0] - bd, 500)
test.expect(test.liveAllocBytes() - d0, 0)
