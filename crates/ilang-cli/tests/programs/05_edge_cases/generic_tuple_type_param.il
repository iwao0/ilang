// Regression guard: monomorphization must substitute a generic type
// parameter nested inside a TUPLE type. `subst_type` recursed into
// Array / Optional / Weak / Generic / Fn but had no `Type::Tuple` arm,
// so a tuple in a generic fn / method signature or field type — `(T, T)`,
// `(T?, T)`, `((T, T), T)` — kept its inner `Object("T")` past
// monomorphization and the lowerer aborted with "unknown type: T".
// (`contains_type_var` had the same gap, so the mangling decision for a
// generic wrapping a tuple was also wrong.)
//
// Optional / array / Map fields of `T` already worked; only tuples were
// missed. As a LOCAL annotation `(T, T)` worked (locals aren't part of
// the monomorphized signature), which is why this hid for so long.
//
// Covers: tuple in a generic fn param + return, a generic class tuple
// FIELD with an Optional slot (so the bare-name field write also has to
// element-wrap T -> T? after monomorphization), and a nested tuple.
// ARC is exact across the heap T = Box.

use std.test as test

let deinits: i64[] = [0]

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

// Tuple in a generic fn signature (param and return).
fn dup<T>(x: T): (T, T) { (x, x) }
fn firstOf<T>(p: (T, T)): T { p[0] }

// Nested tuple in a generic fn return.
fn nest<T>(x: T): ((T, T), T) { ((x, x), x) }

// Generic class with a tuple field whose first slot is Optional, written
// by bare name (exercises element-wrap of the monomorphized T -> T?).
class Cell<T> {
    pair: (T?, T)
    init(seed: T) { this.pair = (none, seed) }
    set(a: T, b: T): i64 {
        pair = (a, b)
        match pair[0] {
            some(_x) { 1 }
            none { 0 }
        }
    }
}

// dup: Box(1) shared into a (Box, Box) pair, then firstOf reads slot 0.
// The pair shares one Box; it dies once. 1 Box/round.
fn useFn(): i64 {
    let b = new Box(1)
    let pair = dup(b)
    let f = firstOf(pair)
    f.n
}

// nest builds ((b, b), b) — all three slots share the one Box. 1/round.
fn useNest(): i64 {
    let b = new Box(2)
    let t = nest(b)
    t[1].n
}

// Cell: seed Box(0) replaced by (some(Box7), Box3). seed dies on the
// bare reassign; Box(7) + Box(3) die when the Cell drops. 3 Box/round.
fn useCell(): i64 {
    let c = new Cell<Box>(new Box(0))
    c.set(new Box(7), new Box(3))
}

let acc: i64[] = [0]

test.expect(useFn(), 1)
test.expect(useNest(), 2)
test.expect(useCell(), 1)

// Per round: useFn 1 + useNest 1 + useCell 3 = 5 deinits.
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    acc[0] = acc[0] + useFn() + useNest() + useCell()
    i = i + 1
}
test.expect(deinits[0] - bd, 500)
test.expect(test.liveAllocBytes() - d0, 0)
