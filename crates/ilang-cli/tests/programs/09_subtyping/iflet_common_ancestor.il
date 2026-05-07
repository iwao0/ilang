// if-let branches with subclass results unify at common ancestor
// (92a2185 — was missing entirely from the if-let join path).
use test

class Animal {
    init() {}
    label(): string { "animal" }
}
class Dog extends Animal {
    init() { super() }
    override label(): string { "dog" }
}

fn classify(opt: i64?): Animal {
    if let some(_) = opt { new Dog() } else { new Animal() }
}

test.expectStr(classify(some(5)).label(), "dog")
test.expectStr(classify(none).label(), "animal")
