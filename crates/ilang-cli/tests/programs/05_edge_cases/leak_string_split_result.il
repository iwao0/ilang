// `s.split(",")` returns `string[]` whose elements are fresh
// `leak_cstring` entries. The result array's drop must
// cascade-release every entry so the registry doesn't grow per
// iteration.
//
// Pre-fix `build_array` always tagged the result as KIND_NONE
// (no element cascade). split-into-array followed by drop left
// the parts forever in `STRING_REGISTRY`. Fix: thread a kind
// param through `build_array`, set KIND_STR for split's output.
//
// jit: skip

use test

let s = "a,b,c,d,e,f,g,h,i,j"

let baseline = test.liveStringCount()

let i: i64 = 0
while i < 1000 {
    let parts = s.split(",")
    let _ = parts
    i = i + 1
}

let after = test.liveStringCount()
let grew = after - baseline

// Pre-fix would leave 1000 × 10 = 10000 string entries in the
// registry. With KIND_STR cascade, each iter's parts release
// at scope exit.
test.expectTrue(grew < 50)
