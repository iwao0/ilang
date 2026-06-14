// Property getters / setters dispatch VIRTUALLY, like regular methods:
// an overridden accessor reached through a base-typed reference runs the
// override, not the base version. Previously properties were resolved
// statically from the receiver's declared type (a direct call to the
// declaring class's accessor), so `(base: Animal = new Dog()).sound`
// returned the Animal getter. The accessors are now wired into the
// vtable (synthetic `name::get` / `name::set` slots), so overrides land
// in the inherited slot and dispatch by the runtime class.
use std.test as test

class Animal {
    init() {}
    get sound(): string { "generic" }
    get value(): i64 { 1 }
    set tag(v: i64) { this.onTag(v) }
    onTag(v: i64) {}
}

class Dog: Animal {
    pub last: i64
    init() { super(); this.last = 0 }
    get sound(): string { "woof" }              // overrides the getter
    get value(): i64 { 2 }
    override onTag(v: i64) { this.last = v * 10 }
}

class Puppy: Dog {
    init() { super() }                          // inherits Dog's overrides
}

// Getter override dispatched through a base-typed reference.
let a: Animal = new Dog()
test.expectStr(a.sound, "woof")
test.expect(a.value, 2)

// Multi-level: Puppy inherits Dog's overrides through the same slot.
let p: Animal = new Puppy()
test.expectStr(p.sound, "woof")
test.expect(p.value, 2)

// Setter override through a base-typed reference.
let d = new Dog()
let ad: Animal = d
ad.tag = 5
test.expect(d.last, 50)

// A base instance still runs the base accessor.
let base: Animal = new Animal()
test.expectStr(base.sound, "generic")
test.expect(base.value, 1)

// Direct (static-type) access remains correct.
test.expectStr((new Dog()).sound, "woof")
