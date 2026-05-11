// `obj.tup = (...)` where the field's type is a tuple — the field-
// store path has to release the previous tuple cell before storing
// the new one. Each iteration replaces an existing tuple, so only
// the FINAL tuple should remain at end-of-loop; every intermediate
// must drop.
//
// `leak_tuple_cell_loop.il` covers the bare `let t = (...)`
// shape; `leak_field_reassign_heap.il` covers the array-field
// case; this fixture is the tuple-field variant.
//

use test

class Pos {
    pt: (i64, i64)
    init() { this.pt = (0, 0) }
}

let baseline = test.liveAllocBytes()

let p = new Pos()
let i: i64 = 0
while i < 300 {
    p.pt = (i, i * 2)
    let _ = p.pt[0]
    i = i + 1
}

let after = test.liveAllocBytes()
test.expectTrue(after - baseline < 32768)
