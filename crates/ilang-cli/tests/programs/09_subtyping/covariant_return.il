// Override with a covariant (subclass) return type (1c75465).
use test

class Animal {
    init() {}
    label(): string { "animal" }
}
class Dog extends Animal {
    init() { super() }
    override label(): string { "dog" }
}

class Factory {
    init() {}
    make(): Animal { new Animal() }
}
class DogFactory extends Factory {
    init() { super() }
    override make(): Dog { new Dog() }
}

let f: Factory = new DogFactory()
test.expectStr(f.make().label(), "dog")
