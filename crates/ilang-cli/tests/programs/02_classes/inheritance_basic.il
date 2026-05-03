// Single inheritance with method override + super dispatch.
// jit: skip
use test

class Animal {
    name: string
    init(n: string) { this.name = n }
    speak(): string { "generic sound" }
    describe(): string { this.name + " says " + this.speak() }
}

class Dog extends Animal {
    init(n: string) { super(n) }
    override speak(): string { "woof" }
}

class Puppy extends Dog {
    init(n: string) { super(n) }
    override speak(): string { "yip!" }
    override describe(): string { "puppy: " + super.describe() }
}

let a = new Animal("rex")
test.expectStr(a.speak(), "generic sound")
test.expectStr(a.describe(), "rex says generic sound")

let d = new Dog("rex")
test.expectStr(d.speak(), "woof")
// Polymorphism: Animal.describe() calls speak() which dispatches
// to Dog's override, not Animal's.
test.expectStr(d.describe(), "rex says woof")

let p = new Puppy("buddy")
test.expectStr(p.speak(), "yip!")
test.expectStr(p.describe(), "puppy: buddy says yip!")

// Subtyping: Dog is-a Animal, can pass where Animal expected.
fn introduce(a: Animal): string { a.describe() }
test.expectStr(introduce(d), "rex says woof")
test.expectStr(introduce(p), "puppy: buddy says yip!")

// Inherited fields are accessible.
test.expectStr(p.name, "buddy")
