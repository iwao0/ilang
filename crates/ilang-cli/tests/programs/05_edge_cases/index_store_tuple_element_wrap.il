// Regression guard: an index-assignment `coll[i] = <composite literal>`
// must build the literal with the container's declared element/value
// type, so a tuple whose element needs wrapping is constructed
// correctly. `AssignIndex` lowered the rhs with no hint and then
// `coerce`d the whole value — but `coerce` only does a `T -> T?` wrap of
// the value itself, not of its inner elements. So
//
//   arr[0] = (box, b)   against (Box?, Box)[]
//   m["k"] = (box, b)   against Map<_, (Box?, Box)>
//
// stored a raw object in the `Box?` slot, and the release cascade walked
// it as an Optional and dereferenced a misaligned pointer -> SIGSEGV.
// (The array/map LITERAL forms `[(box, b)]` / `{"k": (box, b)}` already
// hinted their elements, so only the index-store path crashed.)
//
// Fixed by lowering the rhs through `lower_composite_with_hint` with the
// slot's element/value type, matching the field-write paths. Dispatch
// through each Optional confirms the stored value is the real object.
// ARC exact.

use std.test as test

let deinits: i64[] = [0]

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

// arr[i] = (box, b) into (Box?, Box)[] — slot 0 wraps to Box?.
fn arrStore(): i64 {
    let arr: (Box?, Box)[] = [(none, new Box(0))]
    arr[0] = (new Box(7), new Box(3))          // old (none, Box0) released
    match arr[0][0] {
        some(v) { v.n + arr[0][1].n }
        none { -1 }
    }
}

// m[k] = (box, b) into Map<_, (Box?, Box)> — slot 0 wraps to Box?.
fn mapStore(): i64 {
    let m: Map<string, (Box?, Box)> = new Map<string, (Box?, Box)>()
    m["k"] = (new Box(5), new Box(6))
    match m.get("k") {
        some(t) {
            match t[0] {
                some(v) { v.n + t[1].n }
                none { t[1].n }
            }
        }
        none { -1 }
    }
}

let acc: i64[] = [0]

test.expect(arrStore(), 10)
test.expect(mapStore(), 11)

// Per round: arrStore Box0 + Box7 + Box3 = 3, mapStore Box5 + Box6 = 2.
// Total 5 deinits.
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    acc[0] = acc[0] + arrStore() + mapStore()
    i = i + 1
}
test.expect(deinits[0] - bd, 500)
test.expect(test.liveAllocBytes() - d0, 0)
