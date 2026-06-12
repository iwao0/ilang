// Subclass instance wrapped into `Optional<Parent>` (`T → T?` auto-wrap
// where the source is a subclass of the Optional's inner type). The
// `T → T?` wrap was hard-coded to fire only on an EXACT type match
// (`inner == source`) in three open-coded spots, so a `Dog` flowing
// into an `Animal?` slot was never wrapped:
//
//   - `coerce` (let / arg / return / reassign / tuple-literal /
//     array-literal) raised "no coercion from Dog to Animal" at lower
//     time even though the checker accepts the subtype.
//   - the Map index-assign + array index-assign + field-assign paths
//     stored the RAW object into the Optional slot, so the later
//     `$map.release` / slot release cascade-read it as an Optional cell
//     and dereferenced a garbage inner pointer → SIGABRT.
//   - `release_owned_wrap_source` likewise missed the wrap, so even
//     where the value was correct the fresh source's +1 leaked.
//
// Fix: the wrap predicate in `coerce`, `release_owned_wrap_source`, and
// `AssignField`'s `needs_optional_wrap` all accept a plain
// `Object → Optional<Object>` (subclass) in addition to the exact
// match. An `Optional<_>` source still falls through to the
// Optional→Optional widen, so `none` / `Dog? → Animal?` are unchanged.
//
// Methods return i64 (no string interning) so byte-delta is exactly 0.
// Virtual dispatch through the Optional confirms the runtime class is
// preserved (Dog.val() = n*10). Success is silent + exit 0.

use std.test as test

let deinits: i64[] = [0]

class Animal {
    n: i64
    init(x: i64) { this.n = x }
    val(): i64 { this.n }
    deinit() { deinits[0] = deinits[0] + 1 }
}
class Dog: Animal {
    init(x: i64) { super(x) }
    override val(): i64 { this.n * 10 }
}
class Holder {
    a: Animal
    init(x: i64) { this.a = new Dog(x) }
}
class FH {
    o: Animal?
    init() { this.o = none }
}
enum Box {
    empty
    hold: (Animal?)
}

fn letWrap(): i64 {
    let o: Animal? = new Dog(1)
    if let some(a) = o { a.val() } else { -1 }          // 10
}
fn retWrap(): Animal? { new Dog(2) }
fn useRet(): i64 {
    if let some(a) = retWrap() { a.val() } else { -1 }  // 20
}
fn reassign(): i64 {
    let o: Animal? = none
    o = new Dog(3)
    if let some(a) = o { a.val() } else { -1 }          // 30
}
fn tupWrap(): i64 {
    let t: (Animal?, i64) = (new Dog(4), 0)
    let (o, _k) = t
    if let some(a) = o { a.val() } else { -1 }          // 40
}
fn arrLitWrap(): i64 {
    let arr: Animal?[] = [new Dog(5)]
    if let some(a) = arr[0] { a.val() } else { -1 }     // 50
}
fn arrIdxWrap(): i64 {
    let arr: Animal?[] = [none]
    arr[0] = new Dog(6)
    if let some(a) = arr[0] { a.val() } else { -1 }     // 60
}
fn mapIdxWrap(): i64 {
    let m = new Map<string, Animal?>()
    m["k"] = new Dog(7)
    if let some(a) = m["k"] { a.val() } else { -1 }     // 70
}
fn fieldWrap(): i64 {
    let h = new FH()
    h.o = new Dog(8)
    if let some(a) = h.o { a.val() } else { -1 }        // 80
}
fn enumWrap(): i64 {
    let w = Box.hold(new Dog(9))
    match w {
        empty { -1 }
        hold(o) { if let some(a) = o { a.val() } else { -2 } }   // 90
    }
}
fn borrowedWrap(): i64 {
    let h = new Holder(11)
    let arr: Animal?[] = [h.a]                          // borrowed subclass source
    if let some(a) = arr[0] { a.val() } else { -1 }     // 110
}

// Regression guards: Optional<_> sources must NOT be treated as a wrap.
fn noneInArr(): i64 {
    let arr: Animal?[] = [none, new Dog(1), none]
    let c = 0
    for o in arr { if let some(a) = o { c = c + a.val() } }
    c                                                   // 10 ; 1 deinit
}
fn optWiden(): i64 {
    let od: Dog? = new Dog(1)
    let oa: Animal? = od                                // Optional<Dog> -> Optional<Animal>
    if let some(a) = oa { a.val() } else { -1 }         // 10 ; 1 deinit
}

let acc: i64[] = [0]

// Warm up + verify each value (virtual dispatch through the Optional).
test.expect(letWrap(), 10)
test.expect(useRet(), 20)
test.expect(reassign(), 30)
test.expect(tupWrap(), 40)
test.expect(arrLitWrap(), 50)
test.expect(arrIdxWrap(), 60)
test.expect(mapIdxWrap(), 70)
test.expect(fieldWrap(), 80)
test.expect(enumWrap(), 90)
test.expect(borrowedWrap(), 110)
test.expect(noneInArr(), 10)
test.expect(optWiden(), 10)

// Per round: 12 functions, each makes exactly one Dog that must die
// (borrowedWrap's Dog dies via the Holder), so 12 deinits/round.
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    acc[0] = acc[0]
        + letWrap() + useRet() + reassign() + tupWrap() + arrLitWrap()
        + arrIdxWrap() + mapIdxWrap() + fieldWrap() + enumWrap()
        + borrowedWrap() + noneInArr() + optWiden()
    i = i + 1
}
test.expect(deinits[0] - bd, 1200)
test.expect(test.liveAllocBytes() - d0, 0)
