// `EnumName.payloadVariant(heapVal)` cell drop must cascade-
// release any heap-typed payload (Array / Optional / Object /
// Str / Tuple / Map / Closure). EnumPayload extraction retains
// the heap value (ownership transfer); the cell's release-time
// cascade undoes the cell's own +1 so the rc bookkeeping
// balances on the simple "construct then discard" pattern.
//
// Pre-fix `host_release_enum` freed the cell but didn't walk
// payload kinds, so `Box.holds(arr)` left the inner array
// dangling forever even after the cell was reclaimed.
//

use std.test

enum Box {
    holds: (i64[])
    empty
}

let baseline = test.liveAllocBytes()

let i: i64 = 0
while i < 1000 {
    // Each iter:
    //  - allocates a fresh i64[] (88 B with cap rounded up to 4)
    //  - wraps it in `Box.holds` → 16 B enum cell
    //  - both must reclaim at scope exit
    let b = Box.holds([i, i + 1, i + 2])
    let _ = b
    i = i + 1
}

let after = test.liveAllocBytes()
let grew = after - baseline

// Pre-fix per iter: 16 B cell freed but 88 B array leaked
// → ~88 KB total. With cascade, growth is bounded.
test.expectTrue(grew < 1024)
