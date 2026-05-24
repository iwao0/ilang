// Class fields typed at parent receiving subclass instances and
// subclass-typed composites at init time (cd61814).
use std.test

class Animal {
    init() {}
    label(): string { "animal" }
}
class Dog: Animal {
    init() { super() }
    override label(): string { "dog" }
}

class Box {
    pet: Animal
    pets: Animal[]
    maybe: Animal?
    init() {
        this.pet = new Dog()
        this.pets = [new Dog(), new Dog()]
        this.maybe = some(new Dog())
    }
}

let b = new Box()
test.expectStr(b.pet.label(), "dog")
test.expectStr(b.pets[0].label(), "dog")
test.expectStr(b.maybe.unwrap().label(), "dog")
