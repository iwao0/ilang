// `typeof(x).name` — RTTI on classes and primitives. Per
// syntax.md §13a. Both the direct chain and a let-bound `Type`
// handle resolve to the same string.
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

// Primitives surface their canonical type name.
test.expectStr(typeof(1).name, "i64")
test.expectStr(typeof("hi").name, "string")
test.expectStr(typeof(true).name, "bool")
test.expectStr(typeof(1.0).name, "f64")
test.expectStr(typeof(1_u32).name, "u32")
test.expectStr(typeof(1.0_f32).name, "f32")

// Composite shapes — array / tuple / optional report their
// structural kind name (no element-type detail today).
test.expectStr(typeof([1, 2, 3]).name, "array")
test.expectStr(typeof((1, "a")).name, "tuple")
let opt: i64? = some(5)
test.expectStr(typeof(opt).name, "optional")
