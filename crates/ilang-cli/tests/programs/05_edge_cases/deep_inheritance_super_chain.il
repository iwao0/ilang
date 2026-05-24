// Four-level inheritance with super.f() at every override.
// `inheritance_basic.il` covers two levels (Animal → Dog → Puppy)
// but doesn't pin down whether `super` resolves to the *immediate*
// parent's vtable slot regardless of subclass count — a regression
// that walks the whole chain or stops at the wrong level would
// silently double-count or skip layers.
use std.test

class A { v(): i64 { 1 } }
class B: A { override v(): i64 { 10 + super.v() } }
class C: B { override v(): i64 { 100 + super.v() } }
class D: C { override v(): i64 { 1000 + super.v() } }

let d = new D()
test.expect(d.v(), 1111)         // 1000 + 100 + 10 + 1

// Each level resolves only one super hop.
let c: A = new C()
test.expect(c.v(), 111)

let b: A = new B()
test.expect(b.v(), 11)

let a = new A()
test.expect(a.v(), 1)

// Mixed array — virtual dispatch through A[] sees each subclass's
// fully-evaluated super chain.
let xs: A[] = [new A(), new B(), new C(), new D()]
let total = 0
for x in xs { total += x.v() }
test.expect(total, 1234)         // 1 + 11 + 111 + 1111
