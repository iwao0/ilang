// Nested / composite covariance for fresh container literals, and the
// `T[] -> T[]?` (and friends) Optional auto-wrap. Round 26 widened the
// `T -> T?` wrap to accept a subclass source but restricted it to plain
// `Object -> Optional<Object>`. That left OBJECT-SHAPED-BUT-NOT-OBJECT
// sources out: `Dog[]` into `Animal[]?` (i.e. `Array<Dog> ->
// Optional<Array<Animal>>`) raised "no coercion from obj[] to obj[]?"
// in lowering even though the checker accepts it (a fresh literal is
// covariant), and a store site would have crashed the same way the
// subclass case did before round 26.
//
// Fix: the wrap predicate in `coerce`, `release_owned_wrap_source`, and
// `AssignField`'s `needs_optional_wrap` now accept any object-shaped
// subtype source (Object / Array / Tuple / Map / Optional inner),
// excluding an `Optional<_>` source (that is an Optional->Optional
// widen, not a wrap — keeps `none` and `Box??` unchanged).
//
// Covered: map-of-arrays, nested map, array-of-maps, and the
// `Animal[]?` wrap at let / field / arg / return. i64-returning methods
// (no interning) so byte-delta is exactly 0; virtual dispatch through
// every layer confirms the runtime class survives.

use std.test as test

let deinits: i64[] = [0]

class Animal {
    init() {}
    v(): i64 { 1 }
    deinit() { deinits[0] = deinits[0] + 1 }
}
class Dog: Animal {
    init() { super() }
    override v(): i64 { 10 }
}
class Cat: Animal {
    init() { super() }
    override v(): i64 { 100 }
}
class FH {
    o: Animal[]?
    init() { this.o = none }
}

// Map<string, Animal[]> — value is an array of subclass instances. 2/round.
fn mapOfArr(): i64 {
    let m: Map<string, Animal[]> = {"a": [new Dog(), new Cat()]}
    m["a"][0].v() + m["a"][1].v()                  // 110
}
// Map<string, Map<string, Animal>> — nested map literal. 1/round.
fn nestedMap(): i64 {
    let m: Map<string, Map<string, Animal>> = {"a": {"x": new Dog()}}
    m["a"]["x"].v()                                // 10
}
// Map<string, Animal>[] — array of map literals. 2/round.
fn arrOfMap(): i64 {
    let a: Map<string, Animal>[] = [{"k": new Dog()}, {"k": new Cat()}]
    a[0]["k"].v() + a[1]["k"].v()                  // 110
}
// Map<string, Animal[]?> with a some / none mix — Array<Dog> wrapped
// into Optional<Array<Animal>> (the regression). 1 Dog/round.
fn mapOptArr(): i64 {
    let m: Map<string, Animal[]?> = {"a": [new Dog()], "b": none}
    let s: i64[] = [0]
    for e in m.entries() {
        let (_k, o) = e
        if let some(arr) = o { s[0] = s[0] + arr[0].v() }
    }
    s[0]                                           // 10
}
// `Animal[]?` wrap at field / arg / return. 1 Dog/round each.
fn fieldArrOpt(): i64 {
    let h = new FH()
    h.o = [new Dog()]
    if let some(arr) = h.o { arr[0].v() } else { -1 }   // 10
}
fn takeArrOpt(m: Animal[]?): i64 {
    if let some(a) = m { a[0].v() } else { -1 }
}
fn atArg(): i64 { takeArrOpt([new Dog()]) }            // 10
fn retArrOpt(): Animal[]? { [new Dog()] }
fn atRet(): i64 {
    if let some(a) = retArrOpt() { a[0].v() } else { -1 }  // 10
}

let acc: i64[] = [0]

test.expect(mapOfArr(), 110)
test.expect(nestedMap(), 10)
test.expect(arrOfMap(), 110)
test.expect(mapOptArr(), 10)
test.expect(fieldArrOpt(), 10)
test.expect(atArg(), 10)
test.expect(atRet(), 10)

// Per round: mapOfArr 2 + nestedMap 1 + arrOfMap 2 + mapOptArr 1 +
// fieldArrOpt 1 + atArg 1 + atRet 1 = 9 deinits.
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    acc[0] = acc[0]
        + mapOfArr() + nestedMap() + arrOfMap() + mapOptArr()
        + fieldArrOpt() + atArg() + atRet()
    i = i + 1
}
test.expect(deinits[0] - bd, 900)
test.expect(test.liveAllocBytes() - d0, 0)
