// `xs.push(new Child())` where `xs: Parent[]` (cd61814).
use test

class Animal {
    init() {}
    label(): string { "animal" }
}
class Dog extends Animal {
    init() { super() }
    override label(): string { "dog" }
}

let arr: Animal[] = [new Animal()]
arr.push(new Dog())
test.expect(arr.length, 2)
test.expectStr(arr[0].label(), "animal")
test.expectStr(arr[1].label(), "dog")
