// Regression guard: the implicit bare-name field write `f = v` inside a
// method routed through a degenerate store that ignored the field's
// declared type — it never applied the `T → T?` Optional auto-wrap that
// the explicit `this.f = v` path (AssignField) does. So `slot = box`
// against a `Box?` field stored a *raw* object into the Optional slot;
// on release the cascade walked it as an Optional and dereferenced a
// misaligned pointer -> SIGSEGV.
//
// Fixed by sharing one `store_value_to_field` helper between the
// explicit and bare field-write paths, so both wrap, retain/release,
// and value-copy fixed arrays identically.
//
// Covers: plain `T → T?` wrap, an object-shaped *subtype* source
// (`Dog -> Animal?`, dispatched through the Optional to confirm the
// wrap kept the real object), and the same bare write *inside a
// closure* (which resolves `this` through captures). Values right, ARC
// exact.

use std.test as test

let deinits: i64[] = [0]

class Animal {
    init() {}
    deinit() { deinits[0] = deinits[0] + 1 }
    val(): i64 { 1 }
}

class Dog: Animal {
    init() { super() }
    override val(): i64 { 10 }
}

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

// Bare write of a same-type value into a `Box?` field.
class OptBox {
    slot: Box?
    init() { this.slot = none }
    set(b: Box): i64 {
        slot = b                      // T -> T? wrap, no `this.`
        match slot {
            some(v) { v.n }
            none { -1 }
        }
    }
}

// Bare write of a *subtype* into an `Animal?` field; dispatch through
// the Optional proves the stored value is the real Dog, not a raw blob.
class Kennel {
    pet: Animal?
    init() { this.pet = none }
    put(d: Dog): i64 {
        pet = d                       // Dog -> Animal? (subtype + wrap)
        match pet {
            some(a) { a.val() }
            none { -1 }
        }
    }
}

// Same bare Optional write, but from inside a closure — `this` comes in
// through the capture, exercising both the wrap and the capture path.
class LazyKennel {
    pet: Animal?
    init() { this.pet = none }
    setter(): fn(Dog): i64 {
        fn(d: Dog): i64 {
            pet = d
            match pet {
                some(a) { a.val() }
                none { -1 }
            }
        }
    }
}

// 1 Box/round.
fn optbox(): i64 {
    let o = new OptBox()
    o.set(new Box(7))                 // 7
}
// 1 Dog/round.
fn kennel(): i64 {
    let k = new Kennel()
    k.put(new Dog())                  // 10
}
// 1 Dog/round.
fn lazy(): i64 {
    let k = new LazyKennel()
    let s = k.setter()
    s(new Dog())                      // 10
}

let acc: i64[] = [0]

test.expect(optbox(), 7)
test.expect(kennel(), 10)
test.expect(lazy(), 10)

// Per round: optbox 1 + kennel 1 + lazy 1 = 3 deinits.
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    acc[0] = acc[0] + optbox() + kennel() + lazy()
    i = i + 1
}
test.expect(deinits[0] - bd, 300)
test.expect(test.liveAllocBytes() - d0, 0)
