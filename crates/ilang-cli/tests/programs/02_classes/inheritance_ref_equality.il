// `==` / `!=` on classes is reference equality, even when the
// receivers are typed as a common ancestor. A child stored in a
// parent slot still compares by allocation address, not by
// runtime type. Extends `class_eq_ref.il` with the inheritance
// dimension.
use std.test as test

class Animal {
    name: string
    init(n: string) { this.name = n }
}
class Dog: Animal { init(n: string) { super(n) } }
class Cat: Animal { init(n: string) { super(n) } }

let d1 = new Dog("rex")
let d2 = new Dog("rex")    // same fields, different allocation
let c1 = new Cat("rex")

// Distinct allocations are not equal — even with identical fields.
test.expectBool(d1 == d2, false)
test.expectBool(d1 != d2, true)

// Aliasing the same allocation is equal.
let alias = d1
test.expectBool(d1 == alias, true)

// Comparing through the parent-typed slot still uses identity.
let a1: Animal = d1
let a2: Animal = d2
test.expectBool(a1 == a2, false)
test.expectBool(a1 == d1, true)        // same heap object
test.expectBool(a1 != c1, true)        // unrelated subclasses

// Mixed-type binding — a Dog handed to an `Animal` slot is the
// same object whichever face is presented.
let same_d: Animal = d1
test.expectBool(same_d == d1, true)
test.expectBool(same_d == alias, true)
