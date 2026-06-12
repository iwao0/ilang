// Regression guard: a bare-name field write `f = <composite literal>`
// must construct the literal with the FIELD's declared element types,
// not the inferred ones. The bare path reuses the rhs lowered without a
// type hint (the binding lookup finds no type for a class field), so a
// tuple / array / map literal whose elements need wrapping to the
// declared element type was built with the raw element type instead:
//
//   pair = (box, b)   against (Box?, Box)   -> built as (Box, Box)
//   arr  = [box]      against Box?[]        -> built as Box[]
//   m    = {"k": box} against Map<_, Box?>  -> built as Map<_, Box>
//
// A raw object then sat in a `Box?` slot; the release cascade walked it
// as an Optional and dereferenced a misaligned pointer -> SIGSEGV. The
// explicit `this.f = ...` (AssignField) path always used the field hint,
// so only the bare form crashed.
//
// Fixed by feeding the field's type as the composite-literal hint on the
// bare path too (the outer `T -> T?` wrap is still applied by
// `store_value_to_field`; this fixes the inner shape). Dispatch through
// each Optional confirms the stored value is the real object. ARC exact.

use std.test as test

let deinits: i64[] = [0]

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

// Tuple field with a wrapped slot.
class TupHolder {
    pair: (Box?, Box)
    init() { this.pair = (none, new Box(0)) }
    set(a: Box, b: Box): i64 {
        pair = (a, b)
        match pair[0] {
            some(v) { v.n + pair[1].n }
            none { pair[1].n }
        }
    }
}

// Array field with wrapped elements.
class ArrHolder {
    arr: Box?[]
    init() { this.arr = [] }
    fill(b: Box): i64 {
        arr = [b]
        match arr[0] {
            some(v) { v.n }
            none { -1 }
        }
    }
}

// Map field with wrapped values.
class MapHolder {
    m: Map<string, Box?>
    init() { this.m = new Map<string, Box?>() }
    put(b: Box): i64 {
        m = {"k": b}
        match m.get("k") {
            some(opt) {
                match opt {
                    some(v) { v.n }
                    none { -2 }
                }
            }
            none { -1 }
        }
    }
}

// Boxes per round: init Box(0) + Box(7) + Box(3) = 3, all die.
fn tup(): i64 {
    let h = new TupHolder()
    h.set(new Box(7), new Box(3))         // 10
}
// 1 Box/round.
fn arr(): i64 {
    let a = new ArrHolder()
    a.fill(new Box(7))                    // 7
}
// 1 Box/round.
fn map(): i64 {
    let m = new MapHolder()
    m.put(new Box(9))                     // 9
}

let acc: i64[] = [0]

test.expect(tup(), 10)
test.expect(arr(), 7)
test.expect(map(), 9)

// Per round: tup 3 + arr 1 + map 1 = 5 deinits.
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    acc[0] = acc[0] + tup() + arr() + map()
    i = i + 1
}
test.expect(deinits[0] - bd, 500)
test.expect(test.liveAllocBytes() - d0, 0)
