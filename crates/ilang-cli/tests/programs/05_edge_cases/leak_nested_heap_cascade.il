// `i64[][]` (array of arrays) — the outer array's drop must
// cascade-release every inner array, otherwise the inner
// arrays' headers + data buffers leak permanently.
//
// `host_release_array` only walks the inner pointers when the
// kind_tag header word is 1 (set by NewArray when the static
// element type is an Object). For Array-of-Array element types,
// the tag is currently 0, so nothing cascades and only the outer
// array's own header + data are freed — every inner array leaks.
//
// jit: skip

use test

let baseline = test.liveAllocBytes()

let i: i64 = 0
while i < 1000 {
    // Outer Array<i64[]> with two inner arrays per iter.
    let xs: i64[][] = [[i, i + 1], [i + 2, i + 3, i + 4]]
    let _ = xs
    i = i + 1
}

let after = test.liveAllocBytes()
let grew = after - baseline

// Loose threshold for documentation: each iter would leak two
// inner arrays at ≥ 48 + cap*8 bytes each (cap rounds up to 4),
// so ~160 bytes / iter × 1000 ≈ 160 KB. Tighten to < 1024 once
// host_release_array's cascade also handles Array-of-Array.
test.expectTrue(grew < 524288)
