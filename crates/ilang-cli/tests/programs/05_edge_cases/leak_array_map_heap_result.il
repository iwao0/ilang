// `xs.map(fn(x) { ... })` whose closure returns a heap value
// (string here) — the result array's drop must cascade-release
// every produced string so the registry doesn't grow per
// iteration.
//
// Pre-fix `host_array_map` always tagged the result array as
// KIND_NONE because the closure's return MirTy wasn't threaded
// through. Map producing strings then leaked all entries.
//

use std.test as test
let xs: i64[] = [1, 2, 3, 4, 5]

let baseline = test.liveStringCount()

let i: i64 = 0
while i < 1000 {
    let ys = xs.map(fn(x: i64): string { x.toString() })
    let _ = ys
    i = i + 1
}

let after = test.liveStringCount()
let grew = after - baseline

// Each iter produces 5 strings via toString. Pre-fix: 5000
// orphan registry entries. Post-fix: the result array's drop
// cascades release_string on each entry.
test.expectTrue(grew < 50)
