// Implicit `T → T?` / `T → T.weak` wrap coerce at CONTAINER STORE
// positions that the 第20-22 wrap-coerce rounds missed. Those rounds
// fixed the wrap at `let` / arg / return / reassign / field-assign /
// Map-literal, all of which route the owned-source release through
// `release_owned_wrap_source` (or `lower_arg_to`). Four more store
// sites open-coded their own coerce + retain and never dropped the
// wrapped source's +1 (Map index-assign skipped the coerce entirely):
//
//   1. `m[k] = box`  (Map index-assign against `Map<_, Box?>`)
//        — stored the RAW Box where a `Box?` cell was expected, so
//          `$map.release` cascade-released it under the Optional kind
//          and dereferenced a garbage inner pointer → SIGABRT.
//   2. `arr[i] = box`  (array index-assign against `Box?[]`)
//        — coerced but leaked the source's +1 (fresh) / double-counted
//          the wrapper cell (borrowed).
//   3. `[box]: Box?[]`  (array literal element)         — same leak.
//   4. `(box, _): (Box?, _)`  (tuple literal element)   — same leak.
//
// Fix: each site now detects the wrap (`coerced != source`) and calls
// `release_owned_wrap_source` instead of the aliased-element retain,
// and the Map arm coerces to the declared V first (Map index-assign
// also releases the transient cell left after host_map_set's retain).
//
// Hand-computed exact deinit + 100-round churn delta=0. Counterparts
// that were already correct (`m.set(k, box)` via lower_arg_to, explicit
// `some(..)`, enum payload wrap, field-assign) are kept as regression
// guards. Success is silent (test.expect only) + exit 0.

use std.test as test

let deinits: i64[] = [0]

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

class Holder {
    b: Box
    init(x: i64) { this.b = new Box(x) }
}

class FieldHolder {
    o: Box?
    init() { this.o = none }
}

enum Wrap {
    empty
    hold: (Box?)
}

// 1. Map index-assign (was a crash). 1 Box/round.
fn mapIdx(): i64 {
    let m = new Map<string, Box?>()
    m["a"] = new Box(1)
    if let some(b) = m["a"] { b.n } else { -1 }
}

// 2. Array index-assign wrap. 1 Box/round.
fn arrIdx(): i64 {
    let arr: Box?[] = [none]
    arr[0] = new Box(2)
    if let some(b) = arr[0] { b.n } else { -1 }
}

// 3. Array literal element wrap — fresh AND borrowed source. 2/round
// (the fresh Box(2) and the Holder's Box(3)).
fn arrLit(): i64 {
    let h = new Holder(3)
    let arr: Box?[] = [new Box(2), h.b]
    let s: i64[] = [0]
    for o in arr {
        if let some(b) = o { s[0] = s[0] + b.n }
    }
    s[0]
}

// 4. Tuple literal element wrap. 1 Box/round.
fn tupLit(): i64 {
    let t: (Box?, i64) = (new Box(4), 9)
    let (o, _k) = t
    if let some(b) = o { b.n } else { -1 }
}

// Weak wrap at a container literal / index-assign — same helper path,
// no strong share taken, so the strong owner's drop kills the Box. 1/round.
fn weakArr(): i64 {
    let keep = new Box(6)
    let arr: Box.weak[] = [keep]
    arr[0] = keep
    let u = arr[0].get()
    if let some(b) = u { b.n } else { -1 }
}

// --- regression guards: paths that were already correct ---
fn mapSetMethod(): i64 {            // via lower_arg_to
    let m = new Map<string, Box?>()
    m.set("a", new Box(7))
    if let some(b) = m["a"] { b.n } else { -1 }
}
fn explicitSome(): i64 {            // explicit some(..) cell
    let arr: Box?[] = [some(new Box(8))]
    if let some(b) = arr[0] { b.n } else { -1 }
}
fn enumPayload(): i64 {             // wrap via lower_arg_to (enum ctor arg)
    let w = Wrap.hold(new Box(5))
    match w {
        empty { -1 }
        hold(o) { if let some(b) = o { b.n } else { -2 } }
    }
}
fn fieldAssign(): i64 {             // 第22弾 field-assign fix
    let h = new FieldHolder()
    h.o = new Box(9)
    if let some(b) = h.o { b.n } else { -1 }
}

let acc: i64[] = [0]

// Warm up so first-use string interning is out of the byte-delta window.
test.expect(mapIdx(), 1)
test.expect(arrIdx(), 2)
test.expect(arrLit(), 5)
test.expect(tupLit(), 4)
test.expect(weakArr(), 6)
test.expect(mapSetMethod(), 7)
test.expect(explicitSome(), 8)
test.expect(enumPayload(), 5)
test.expect(fieldAssign(), 9)

// Per round: 1 + 1 + 2 + 1 + 1 + 1 + 1 + 1 + 1 = 10 deinits.
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    acc[0] = acc[0]
        + mapIdx() + arrIdx() + arrLit() + tupLit() + weakArr()
        + mapSetMethod() + explicitSome() + enumPayload() + fieldAssign()
    i = i + 1
}
test.expect(deinits[0] - bd, 1000)
test.expect(test.liveAllocBytes() - d0, 0)
