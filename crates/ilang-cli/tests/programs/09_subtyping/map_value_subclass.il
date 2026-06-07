// Map<K, Parent> accepts any subclass value — the value slot
// keeps its runtime class, so reading back through the parent
// face still virtual-dispatches to the right override.
use std.test as test

class Animal {
    init() {}
    kind(): string { "animal" }
}
class Dog: Animal {
    init() { super() }
    override kind(): string { "dog" }
}
class Cat: Animal {
    init() { super() }
    override kind(): string { "cat" }
}

let m: Map<string, Animal> = new Map<string, Animal>()
m["d"] = new Dog()
m["c"] = new Cat()
m["a"] = new Animal()

test.expect(m.size(), 3)
test.expectStr(m["d"].kind(), "dog")
test.expectStr(m["c"].kind(), "cat")
test.expectStr(m["a"].kind(), "animal")

// Iterate via entries — every value is the same Animal-typed slot
// but the runtime class is preserved.
let kinds: string[] = []
for e in m.entries() {
    kinds.push(e[1].kind())
}
test.expect(kinds.length, 3)

// Iterating values directly also keeps virtual dispatch alive.
let n_dog = 0
let n_cat = 0
let n_anim = 0
for v in m.values() {
    if v.kind() == "dog" { n_dog += 1 }
    if v.kind() == "cat" { n_cat += 1 }
    if v.kind() == "animal" { n_anim += 1 }
}
test.expect(n_dog, 1)
test.expect(n_cat, 1)
test.expect(n_anim, 1)
