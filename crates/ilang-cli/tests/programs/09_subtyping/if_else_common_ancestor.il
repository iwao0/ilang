// if/else arms returning sibling subclasses unify at their nearest
// common ancestor (a5e9e2c, 92a2185).
use std.test

class Animal {
    init() {}
    sound(): string { "?" }
}
class Dog: Animal {
    init() { super() }
    override sound(): string { "woof" }
}
class Cat: Animal {
    init() { super() }
    override sound(): string { "meow" }
}

fn pick(b: bool): Animal {
    if b { new Dog() } else { new Cat() }
}

test.expectStr(pick(true).sound(), "woof")
test.expectStr(pick(false).sound(), "meow")
