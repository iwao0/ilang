// A `Parent.weak` slot can hold a value that started life as a
// `Child` instance — the type checker only auto-downgrades on a
// matching class, so the strong reference is upcast to the parent
// type first and the weak slot then sees the parent face. The
// underlying allocation is the same, and `.get()` still routes
// methods through the runtime class's vtable.
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
// Auto-downgrade is same-class only — explicit upcast first.
let parent_strong: Animal = strong
let w: Animal.weak = parent_strong

let g = w.get()
test.expectBool(g.isSome, true)
test.expectStr(g.unwrap().kind(), "dog")        // virtual dispatch finds Dog

// Method dispatch through the weak Animal slot finds the runtime
// Dog impl as long as a strong holder exists.
{
    let inner = w.get()
    test.expectBool(inner.isSome, true)
    test.expectStr(inner.unwrap().kind(), "dog")
}
// Re-binding the underlying allocation via the parent slot keeps
// the weak target alive (parent_strong still owns it).
test.expectBool(w.get().isSome, true)
