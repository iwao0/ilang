// `super.method(...)` resolves to the immediate parent's version
// (statically up the chain). A grandchild calling `super.label()`
// hits the parent's override, which in turn can call `super.label()`
// to reach the grandparent. Per syntax.md §7 "super.method(args)
// calls the parent's version".
use std.test as test

class A {
    init() {}
    label(): string { "A" }
}

class B: A {
    init() { super() }
    override label(): string {
        // Reach A.label via one super-hop.
        super.label() + ">B"
    }
}

class C: B {
    init() { super() }
    override label(): string {
        // Reach B.label via one super-hop — which itself reaches A.
        super.label() + ">C"
    }
}

let c = new C()
test.expectStr(c.label(), "A>B>C")

// Cross-hierarchy: an A-typed view of `c` still dispatches to C's
// override (virtual), but C's body explicitly threads through the
// chain via super.
let a: A = c
test.expectStr(a.label(), "A>B>C")

// B's own value sees its own implementation, which super-hops once.
let b = new B()
test.expectStr(b.label(), "A>B")
