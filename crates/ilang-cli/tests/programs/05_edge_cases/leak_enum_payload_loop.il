// `EnumName.payloadVariant(args)` allocates a `[tag | payload...]`
// cell of size `(1 + n_payload) * 8`. The cell is registered in
// `ENUM_REGISTRY` (rc + total_bytes) so `host_release_enum` can
// free it when rc reaches 0.
//
// Unit-variant constructions go through the `__enum_unit_get`
// cache and don't allocate per call (covered by
// `leak_enum_unit_loop.il`). Heap content inside the payload
// (e.g. `holds(arr)` where arr is an Array) is NOT cascaded
// yet — see TODO in `host_release_enum`.
//

use std.test

enum Box {
    holds: (i64)
    empty
}

let baseline = test.liveAllocBytes()

let i: i64 = 0
while i < 1000 {
    // Each iter allocates a fresh 16-byte cell (tag + 1 i64
    // payload). With cell free in place, growth is bounded.
    let b = Box.holds(i)
    let _ = b
    i = i + 1
}

let after = test.liveAllocBytes()
let grew = after - baseline

// Pre-fix would leak ~16 KB. With cell free, growth is bounded
// by allocator overhead.
test.expectTrue(grew < 1024)
