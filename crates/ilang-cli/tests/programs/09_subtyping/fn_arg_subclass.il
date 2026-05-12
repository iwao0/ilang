// Subclass values flowing into fn args of parent / parent-composite
// types (cd61814).
use test

class Animal {
    init() {}
    label(): string { "animal" }
}
class Dog: Animal {
    init() { super() }
    override label(): string { "dog" }
}

fn label_one(a: Animal): string { a.label() }
fn label_first(xs: Animal[]): string { xs[0].label() }
fn label_opt(o: Animal?): string {
    if let some(a) = o { a.label() } else { "none" }
}

test.expectStr(label_one(new Dog()), "dog")
test.expectStr(label_first([new Dog(), new Dog()]), "dog")
test.expectStr(label_opt(some(new Dog())), "dog")
