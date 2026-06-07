// `Optional<Parent>` with both `some(Child)` and `none` flowing
// through the same slot. `optional_subclass.il` exercises the
// some-only direction; this fixture covers `none`, function
// parameter / return positions, and the match arm dispatch.
use std.test as test

class Animal {
    init() {}
    kind(): string { "animal" }
}
class Dog: Animal {
    init() { super() }
    override kind(): string { "dog" }
}
class Cat: Animal {
    init() { super() }
    override kind(): string { "cat" }
}

fn describe(o: Animal?): string {
    match o {
        some(a) { a.kind() }
        none { "no-animal" }
    }
}

test.expectStr(describe(none), "no-animal")
test.expectStr(describe(some(new Dog())), "dog")
test.expectStr(describe(some(new Cat())), "cat")
test.expectStr(describe(some(new Animal())), "animal")

// Direct slot, both shapes.
let n: Animal? = none
let s: Animal? = some(new Dog())
test.expectBool(n.isNone, true)
test.expectBool(s.isSome, true)
test.expectStr(s.unwrap().kind(), "dog")

// Reassign across child types — the slot stays `Animal?`.
let slot: Animal? = some(new Dog())
test.expectStr(slot.unwrap().kind(), "dog")
slot = some(new Cat())
test.expectStr(slot.unwrap().kind(), "cat")
slot = none
test.expectBool(slot.isNone, true)

// fn that returns `Animal?` — the caller binding sees the
// runtime class through the optional shell.
fn pick(want: string): Animal? {
    if want == "dog" { some(new Dog()) }
    elif want == "cat" { some(new Cat()) }
    else { none }
}
test.expectStr(describe(pick("dog")), "dog")
test.expectStr(describe(pick("cat")), "cat")
test.expectStr(describe(pick("nope")), "no-animal")
