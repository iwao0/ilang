// Per-iteration tuple construction allocates a `[rc | packed |
// elements...]` cell. The cell must be freed when its rc reaches
// 0 — without it, every `let t = (x, y)` in a loop leaks the
// header + element slots.
//
// Pre-fix `host_release_tuple` cascaded Object-typed elements'
// release but never called `host_mir_free` on the cell itself
// (the header lacked an arity word, so the size wasn't known).
// Fix packs `arity` into the upper 32 bits of the kind_mask
// header word.
//

use test

let baseline = test.liveAllocBytes()

let i: i64 = 0
while i < 1000 {
    // Tuple cell: 16 (header) + 2 * 8 (elements) = 32 bytes.
    let t: (i64, i64) = (i, i + 1)
    let _ = t
    i = i + 1
}

let after = test.liveAllocBytes()
let grew = after - baseline

// 1000 tuple cells × 32 bytes = ~32 KB pre-fix. With cell free
// in place, growth is bounded by allocator overhead.
test.expectTrue(grew < 256)
