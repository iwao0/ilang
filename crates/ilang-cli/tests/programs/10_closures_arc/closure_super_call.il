// `super.method(...)` inside a closure built in a child method
// (9475956). Both interp and JIT restore the lexical class so
// the parent's method is reachable.
use std.test as test
class Base {
    init() {}
    label(): string { "base" }
}
class Child: Base {
    init() { super() }
    override label(): string { "child" }
    parent_label(): fn(): string {
        fn(): string { super.label() }
    }
}

let c = new Child()
let g = c.parent_label()
test.expectStr(g(), "base")
test.expectStr(c.label(), "child")
