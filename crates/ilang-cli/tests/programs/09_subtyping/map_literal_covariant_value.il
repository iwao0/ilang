// A map literal flows into a `Map<K, Parent>` / `Map<K, Parent?>` slot
// the same way an array literal flows into `Parent[]`. The checker used
// to infer the value type from the entries only (`{"a": new Dog()}` =>
// `Map<string, Dog>`) and reject the assignment to `Map<string,
// Animal>`, even though the array equivalent `[new Dog()]: Animal[]`
// worked and the lowering already builds the map at the declared K/V
// (`lower_map_literal_with_hint`). The "no Map coerce" comment guarding
// it had gone stale.
//
// Fix: `literal_assignable_with` gained a MapLit arm (each key/value
// checked against the annotation's K/V with the same subclass + literal
// coercions arrays use — covers let / arg / return), and the let
// binding routes a `Map<K,V>`-annotated literal through a new
// `check_map_lit_with_hint` so a `some(child)` / `none` MIX unifies to
// the parent (mirrors `check_array_with_hint`).
//
// Covariance is literal-only: an aliased `Map<string, Dog>` variable
// still does NOT assign to `Map<string, Animal>` (mutation would be
// unsound) — pinned by `map_literal_covariant_value_alias_error.il`.
//
// Methods return i64 (no interning) so byte-delta is exactly 0. Virtual
// dispatch through the parent slot confirms the runtime class survives.

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

// Homogeneous subclass values -> Map<_, Animal>. 2 deinits/round.
fn homo(): i64 {
    let m: Map<string, Animal> = {"a": new Dog(), "b": new Cat()}
    m["a"].v() + m["b"].v()                       // 110
}
// Subclass value -> Map<_, Animal?> (bare child auto-wraps). 1/round.
fn optWrap(): i64 {
    let m: Map<string, Animal?> = {"a": new Dog()}
    if let some(a) = m["a"] { a.v() } else { -1 } // 10
}
// Mixed some(child) / none -> Map<_, Animal?> (needs the hinted let
// path; the inferred path can't unify Dog with none). 2 dogs/round.
fn mixed(): i64 {
    let m: Map<string, Animal?> = {"a": new Dog(), "b": none, "c": new Cat()}
    let s: i64[] = [0]
    for e in m.entries() {
        let (_k, o) = e
        if let some(a) = o { s[0] = s[0] + a.v() }
    }
    s[0]                                          // 110
}
// Covariance also holds at arg / return position (via the
// literal_assignable_with MapLit arm). 1/round each.
fn takeArg(m: Map<string, Animal>): i64 { m["x"].v() }
fn atArg(): i64 { takeArg({"x": new Dog()}) }     // 10
fn atRet(): Map<string, Animal> { {"y": new Cat()} }
fn useRet(): i64 { atRet()["y"].v() }             // 100

let acc: i64[] = [0]

test.expect(homo(), 110)
test.expect(optWrap(), 10)
test.expect(mixed(), 110)
test.expect(atArg(), 10)
test.expect(useRet(), 100)

// Per round: homo 2 + optWrap 1 + mixed 2 + atArg 1 + useRet 1 = 7.
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    acc[0] = acc[0] + homo() + optWrap() + mixed() + atArg() + useRet()
    i = i + 1
}
test.expect(deinits[0] - bd, 700)
test.expect(test.liveAllocBytes() - d0, 0)
