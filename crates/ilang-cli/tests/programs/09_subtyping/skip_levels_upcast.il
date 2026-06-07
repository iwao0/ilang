// A grandchild can flow into a slot typed as any ancestor — not
// just the immediate parent. The vtable still routes calls to
// the runtime class's override. Per syntax.md §7 "Subtyping".
use std.test as test

class A {
    init() {}
    label(): string { "A" }
}
class B: A {
    init() { super() }
    override label(): string { "B" }
}
class C: B {
    init() { super() }
    override label(): string { "C" }
}
class D: C {
    init() { super() }
    override label(): string { "D" }
}

// Skip-3 upcast: D-instance into A slot.
let a: A = new D()
test.expectStr(a.label(), "D")

// Skip-2 upcast: D into B.
let b: B = new D()
test.expectStr(b.label(), "D")

// Skip-2 upcast: C into A.
let a2: A = new C()
test.expectStr(a2.label(), "C")

// fn parameter typed as the grandparent receives any descendant.
fn describe(x: A): string { x.label() }
test.expectStr(describe(new D()), "D")
test.expectStr(describe(new C()), "C")
test.expectStr(describe(new B()), "B")
test.expectStr(describe(new A()), "A")

// Mixed array typed as the root sees each instance's runtime class.
let xs: A[] = []
xs.push(new A())
xs.push(new B())
xs.push(new C())
xs.push(new D())
let joined: string[] = []
for x in xs { joined.push(x.label()) }
test.expectStr(joined.join(","), "A,B,C,D")
