// ARC balance for `is` / `as?`. Three pinned bugs: DowncastOrNone
// boxed into a bare 8-byte [value] cell with no rc / kind tag —
// nothing ever released it (8 bytes leaked per hit) and a release
// would have read past the alloc; the cell also didn't retain the
// inner value. It now mints a regular 24-byte Optional cell
// [value | rc | kind=Object] owning a +1 (mirrors WeakUpgrade), and
// `x as? C` is classified fresh. Fresh operands (`makeB() is B`,
// `makeB() as? B`) drop their transient +1 after the test.

use std.test as test

class A {
    a: i64
    init() { this.a = 1 }
}
class B: A {
    items: i64[]
    init() { super(); this.items = [1, 2, 3] }
}

fn makeB(): A { new B() }
fn makeA(): A { new A() }

// hit / miss / upcast semantics
let x: A = new B()
test.expectBool(x is B, true)
test.expectBool(x is A, true)
let y: A = new A()
test.expectBool(y is B, false)
if let some(_) = y as? B { test.fail("downcast of plain A must miss") }

// downcast result survives source overwrite (the cell owns a +1)
let kept = (x as? B).unwrap()
x = new A()
test.expect(kept.items.length, 3)

// churn: bindings, transient reads, fresh operands — all balance
let base = test.liveAllocBytes()
let i = 0
while i < 200 {
    let v: A = new B()
    test.expectBool(v is B, true)
    if let some(b) = v as? B { test.expect(b.items.length, 3) }
    test.expectBool(makeB() is B, true)
    test.expectBool(makeA() is B, false)
    if let some(fb) = makeB() as? B { test.expect(fb.a, 1) }
    if let some(_) = makeA() as? B { test.fail("must miss") }
    let held = (v as? B).unwrap()
    v = new A()
    test.expect(held.items.length, 3)
    i = i + 1
}
let after = test.liveAllocBytes()
test.expectTrue(after - base < 1024)
