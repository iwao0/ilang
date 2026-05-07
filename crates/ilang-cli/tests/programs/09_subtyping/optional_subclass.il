// `Parent? ← some(new Child())` and the `some(complex_literal)`
// recursion case (29363ed).
use test

class Animal {
    init() {}
    label(): string { "animal" }
}
class Dog extends Animal {
    init() { super() }
    override label(): string { "dog" }
}

let o: Animal? = some(new Dog())
test.expectStr(o.unwrap().label(), "dog")

// Recursive descent: Optional<A[]> ← some([new Child()])
let oarr: Animal[]? = some([new Dog(), new Dog()])
let arr = oarr.unwrap()
test.expect(arr.length, 2)
test.expectStr(arr[0].label(), "dog")
