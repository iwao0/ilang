// Subclass values returned from a fn that declares the parent
// type. The body's tail is checked through `value_assignable` so
// composite literals (Array, Tuple, Optional) flow through too
// (cd61814).
use std.test

class Animal {
    init() {}
    label(): string { "animal" }
}
class Dog: Animal {
    init() { super() }
    override label(): string { "dog" }
}

fn make(): Animal { new Dog() }
fn make_arr(): Animal[] { [new Dog(), new Dog()] }
fn make_opt(): Animal? { some(new Dog()) }

test.expectStr(make().label(), "dog")
test.expectStr(make_arr()[0].label(), "dog")
test.expectStr(make_opt().unwrap().label(), "dog")
