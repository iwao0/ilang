// Repeated `EnumName.unitVariant` in a hot loop must not allocate
// a fresh 8-byte cell per construction. The unit-variant cache
// (3c73a64) interns one cell per (global_enum_id, discriminant)
// and returns the same pointer on every call.
//
// Pre-fix breakout's `Input.poll()` constructed ~14 unit-variant
// enums per frame (`sdl.Axis.leftX`, `sdl.Button.dpadLeft`, …)
// which leaked ~840 cells/sec into the host_mir_alloc tracker.
//

use std.test

enum Color { red, green, blue }

let baseline = test.liveAllocCount()

let i: i64 = 0
while i < 1000 {
    // Bind the enum value so it's actually materialised — the
    // optimizer can't eliminate the construction.
    let c = Color.red
    let _ = c
    i = i + 1
}

let after = test.liveAllocCount()
let grew = after - baseline
// With the cache: at most a tiny fixed delta (the cache entry
// itself + any one-shot setup). Without the cache 1000 fresh
// 8-byte cells would land in the tracker.
test.expectTrue(grew < 20)
