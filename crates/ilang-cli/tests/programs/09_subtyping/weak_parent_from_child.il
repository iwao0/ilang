// A `Parent.weak` slot can hold a value that started life as a
// `Child` instance — the auto-downgrade combines the upcast and
// the weak demotion in one step. The underlying allocation is the
// same i64 heap pointer; `.get()` still routes methods through
// the runtime class's vtable.
use std.test as test

class Animal {
    init() {}
    kind(): string { "animal" }
}
class Dog: Animal {
    init() { super() }
    override kind(): string { "dog" }
}

let strong: Dog = new Dog()
let w: Animal.weak = strong           // subtype + auto-downgrade

let g = w.get()
test.expectBool(g.isSome, true)
test.expectStr(g.unwrap().kind(), "dog")        // virtual dispatch finds Dog

// Method dispatch through the weak Animal slot keeps finding the
// runtime Dog impl as long as a strong holder exists.
{
    let inner = w.get()
    test.expectBool(inner.isSome, true)
    test.expectStr(inner.unwrap().kind(), "dog")
}

// Direct subclass weak from a freshly constructed Dog too.
let w2: Animal.weak = new Dog()
// `new Dog()` is the only owner — letting the literal die in this
// statement; only checking that the type checker / lowering accept
// the direct subclass weak slot.
let _ = w2
