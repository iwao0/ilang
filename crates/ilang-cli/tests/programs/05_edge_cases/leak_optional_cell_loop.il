// `some(value)` allocates a 24-byte Optional cell. The cell must
// be freed when its rc reaches 0 — without it, every per-frame
// `let opt = some(x)` leaks 24 bytes.
//
// Pre-fix `host_release_optional` cascaded the inner release but
// never called `host_mir_free` on the cell itself. Held back
// originally over a some(aliased) double-release concern, which
// was resolved by the Some-retain change in 192b91d.
//

use std.test as test
let baseline = test.liveAllocBytes()

let i: i64 = 0
while i < 1000 {
    let opt: i64? = some(i)
    let _ = opt
    i = i + 1
}

let after = test.liveAllocBytes()
let grew = after - baseline

// 1000 Optional cells × 24 bytes = ~24 KB pre-fix. With the cell
// free in place, growth is bounded by allocator overhead and any
// in-flight cell from the final iteration.
test.expectTrue(grew < 256)
