// Tuple literal element subclass — `(Parent, Parent) ← (Child, Child)`
// (cd61814 + 32111f1).
use test

class Animal {
    init() {}
    label(): string { "animal" }
}
class Dog: Animal {
    init() { super() }
    override label(): string { "dog" }
}

let t: (Animal, Animal) = (new Dog(), new Dog())
test.expectStr(t[0].label(), "dog")
test.expectStr(t[1].label(), "dog")
