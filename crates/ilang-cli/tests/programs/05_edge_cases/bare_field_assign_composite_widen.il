// Confirmation-round pin (第34弾): bare-name field writes `f = expr`
// that need the field's declared type beyond the plain `T -> T?` wrap
// fixed in 第33弾. After unifying the explicit and bare field-store
// paths through `store_value_to_field`, these shapes are all sound;
// this fixture locks them so a future change to the bare path can't
// silently regress them.
//
// Shapes covered (all by bare name, no `this.`):
//   * covariant Map literal   `animals = {"d": new Dog()}`   (Dog -> Animal value)
//   * covariant array literal `members = [new Dog(), ...]`   (Dog[] -> Animal[])
//   * fresh Optional rhs      `slot = makeOpt(7)`            (Box? -> Box?, no double wrap)
//   * Optional widen          `pet = d`                      (Dog? -> Animal?)
//
// Dispatch through the container / Optional confirms the stored value
// is the real object (covariance kept the heap identity). ARC exact.

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

fn makeOpt(k: i64): Box? { some(new Box(k)) }

class Zoo {
    animals: Map<string, Animal>
    init() { this.animals = new Map<string, Animal>() }
    stock(): i64 {
        animals = {"d": new Dog()}
        match animals.get("d") {
            some(a) { a.val() }
            none { -1 }
        }
    }
}

class Pack {
    members: Animal[]
    init() { this.members = [] }
    fill(): i64 {
        members = [new Dog(), new Dog()]
        members[0].val() + members[1].val()
    }
}

class OptCell {
    slot: Box?
    init() { this.slot = none }
    setFresh(): i64 {
        slot = makeOpt(7)
        match slot {
            some(v) { v.n }
            none { -1 }
        }
    }
}

class Widen {
    pet: Animal?
    init() { this.pet = none }
    take(d: Dog?): i64 {
        pet = d
        match pet {
            some(a) { a.val() }
            none { -1 }
        }
    }
}

fn zoo(): i64 { let z = new Zoo(); z.stock() }                 // 10, 1 Dog
fn pack(): i64 { let p = new Pack(); p.fill() }                // 20, 2 Dogs
fn optcell(): i64 { let o = new OptCell(); o.setFresh() }      // 7,  1 Box
fn widen(): i64 {
    let d: Dog? = some(new Dog())
    let w = new Widen()
    w.take(d)                                                  // 10, 1 Dog
}

let acc: i64[] = [0]

test.expect(zoo(), 10)
test.expect(pack(), 20)
test.expect(optcell(), 7)
test.expect(widen(), 10)

// Per round: zoo 1 + pack 2 + optcell 1 + widen 1 = 5 deinits.
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    acc[0] = acc[0] + zoo() + pack() + optcell() + widen()
    i = i + 1
}
test.expect(deinits[0] - bd, 500)
test.expect(test.liveAllocBytes() - d0, 0)
