// `typeof(x).name` — RTTI on a class instance. Per syntax.md §13a.
// Both the direct chain and a let-bound `Type` handle resolve to
// the same string.
//
// Primitive scrutinees (`typeof(1)` / `typeof("s")` / `typeof(true)`)
// are accepted by the type checker but `TypeOf` lowering currently
// assumes a heap object header — primitive RTTI is not exercised
// here.
use std.test as test

class Animal {
    name: string
    init(n: string) { this.name = n }
}

class Dog: Animal {
    init(n: string) { super.init(n) }
}

class Cat: Animal {
    init(n: string) { super.init(n) }
}

// Direct chain.
test.expectStr(typeof(new Dog("x")).name, "Dog")
test.expectStr(typeof(new Animal("x")).name, "Animal")
test.expectStr(typeof(new Cat("y")).name, "Cat")

// Let-bound handle reads the same.
let t1 = typeof(new Dog("y"))
test.expectStr(t1.name, "Dog")

// Reassign the slot to a different Type handle.
let t = typeof(new Animal("a"))
test.expectStr(t.name, "Animal")
t = typeof(new Cat("b"))
test.expectStr(t.name, "Cat")
