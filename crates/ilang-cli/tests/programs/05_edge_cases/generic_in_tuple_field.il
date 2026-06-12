// Regression guard: a CONCRETE generic instantiation nested inside a
// tuple type must be mangled to its monomorphized `Object` name.
//
// 第39弾 added a `Type::Tuple` arm to `subst_type` / `contains_type_var`
// (type-PARAM substitution), but the sibling rewrites that mangle an
// already-concrete `Inner<i64>` / `Maybe<i64>` to its instance name were
// still missing the tuple arm:
//   * `rewrite_type`            (user generic classes)
//   * `rewrite_enum_refs_in_type` (generic enums)
//   * `walk_types_pre`          (discovery of nested instantiations)
// So `(Inner<i64>, i64)` as a field / param / return type kept its inner
// `Type::Generic`, which reached lowering as "unsupported in M1:
// user-defined generic types". A `(T, T)` LOCAL worked (its binding type
// is re-derived from the rhs), and a plain `Inner<i64>` field worked
// (no tuple), which is why this hid behind 第39弾.
//
// Covers a generic CLASS and a generic ENUM, both holding a heap `Box`,
// nested in a tuple field, plus the fn param/return positions. ARC exact.

use std.test as test

let deinits: i64[] = [0]

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

class Inner<T> {
    v: T
    init(x: T) { this.v = x }
    get(): T { this.v }
}

enum Maybe<T> {
    nope
    yep: (T)
}

// Generic class in a tuple field.
class HolderC {
    slot: (Inner<Box>, i64)
    init(a: Inner<Box>) { this.slot = (a, 5) }
    read(): i64 { this.slot[0].get().n }
}

// Generic enum in a tuple field.
class HolderE {
    slot: (Maybe<Box>, i64)
    init(m: Maybe<Box>) { this.slot = (m, 5) }
    read(): i64 {
        match this.slot[0] {
            yep(v) { v.n }
            nope { -1 }
        }
    }
}

// Generic class in a tuple as fn param + return.
fn makeC(x: Inner<i64>): (Inner<i64>, i64) { (x, 3) }
fn firstC(p: (Inner<i64>, i64)): i64 { p[0].get() }

// 1 Box/round.
fn classField(): i64 {
    let h = new HolderC(new Inner<Box>(new Box(7)))
    h.read()
}
// 1 Box/round.
fn enumField(): i64 {
    let h = new HolderE(Maybe.yep(new Box(9)))
    h.read()
}
// no Box (i64 payload), just exercises the param/return tuple lowering.
fn fnPos(): i64 {
    let p = makeC(new Inner<i64>(11))
    firstC(p)
}

let acc: i64[] = [0]

test.expect(classField(), 7)
test.expect(enumField(), 9)
test.expect(fnPos(), 11)

// Per round: classField 1 + enumField 1 = 2 deinits (fnPos has no Box).
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    acc[0] = acc[0] + classField() + enumField() + fnPos()
    i = i + 1
}
test.expect(deinits[0] - bd, 200)
test.expect(test.liveAllocBytes() - d0, 0)
