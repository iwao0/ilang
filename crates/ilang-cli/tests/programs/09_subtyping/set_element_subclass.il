// Set<Parent> accepts any subclass element provided the parent
// satisfies the value-equality protocol (`@derive(Eq, Hash)` here).
// The runtime class is preserved on read-back via `values()`.
use std.test as test

@derive(Eq, Hash)
class Animal {
    name: string
    init(n: string) { this.name = n }
    kind(): string { "animal" }
}

class Dog: Animal {
    init(n: string) { super(n) }
    override kind(): string { "dog" }
}

class Cat: Animal {
    init(n: string) { super(n) }
    override kind(): string { "cat" }
}

let s: Set<Animal> = new Set<Animal>()
s.add(new Dog("rex"))
s.add(new Cat("mike"))
s.add(new Animal("anon"))

test.expect(s.size(), 3)

// values() returns Animal[] — each element still answers `.kind()`
// from its own override.
let labels: string[] = []
for v in s.values() {
    labels.push(v.kind())
}
test.expect(labels.length, 3)

// Two Dogs with the same `name` are deduplicated by the derived
// equals/hashCode (which compares the inherited `name` field).
s.add(new Dog("rex"))
test.expect(s.size(), 3)

// A different-name Dog adds normally.
s.add(new Dog("buddy"))
test.expect(s.size(), 4)
