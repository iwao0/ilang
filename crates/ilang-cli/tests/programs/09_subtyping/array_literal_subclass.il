// Array literal with subclass elements flowing into Parent[]
// (cd61814 + 32111f1: thread subclass through composite literals).
use test

class Animal {
    init() {}
    label(): string { "animal" }
}
class Dog extends Animal {
    init() { super() }
    override label(): string { "dog" }
}

let arr: Animal[] = [new Dog(), new Dog()]
test.expect(arr.length, 2)
test.expectStr(arr[0].label(), "dog")
test.expectStr(arr[1].label(), "dog")
