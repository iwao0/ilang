// `let x: Parent = new Child()` — implicit upcast on the let
// annotation. Was rejected by the TC's let arm before fb16dfc.
use std.test as test
class Animal {
    n: i64
    init() { this.n = 1 }
    label(): string { "animal" }
}
class Dog: Animal {
    init() { super() }
    override label(): string { "dog" }
}

let a: Animal = new Dog()
test.expect(a.n, 1)
test.expectStr(a.label(), "dog")     // virtual dispatch picks Dog's
