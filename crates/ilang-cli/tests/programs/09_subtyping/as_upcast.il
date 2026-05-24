// Explicit `as` upcast — `b as Parent` where `B extends Parent`
// (475cd79).
use std.test

class Animal {
    init() {}
    label(): string { "animal" }
}
class Dog: Animal {
    init() { super() }
    override label(): string { "dog" }
}

let b = new Dog()
let a: Animal = b as Animal
test.expectStr(a.label(), "dog")     // virtual dispatch still hits Dog
