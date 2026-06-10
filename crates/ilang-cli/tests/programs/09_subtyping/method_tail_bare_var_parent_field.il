// Subclass method body whose tail is the bare name of a heap field
// **declared on the parent**. The bare-var → implicit `this.field`
// path runs through the same lower as `Box<T>.get(): T { x }`, but
// `this_class` is the child class. Exercises both inheritance and
// the borrow-tail retain.
use std.test as test

class Item {
    n: i64
    init(x: i64) { this.n = x }
}

class Base {
    held: Item
    init(it: Item) { this.held = it }
}

class Child: Base {
    init(it: Item) { super(it) }
    fetch(): Item { held }
}

let base_bytes = test.liveAllocBytes()
let i = 0
while i < 200 {
    let it = new Item(i + 1)
    let c = new Child(it)
    test.expect(c.fetch().n, i + 1)
    i = i + 1
}
let after = test.liveAllocBytes()
test.expectTrue(after - base_bytes < 2048)
