// Per-iteration object construction allocates a `[class_id | rc |
// fields...]` cell. Currently `release_object` cascades field
// release but does NOT call `host_mir_free` on the cell — the
// guard is intentional, deferred until callee_retain (192b91d)
// is verified across all return-this patterns.
//
// This fixture establishes a baseline: with the object-cell
// free disabled, 1000 instances of a 1-field class leak ~24 KB
// (16-byte header + 1 field). The threshold is set generously
// so the test passes today; once `release_object` learns to
// free, tighten the threshold to detect any regression that
// brings the leak back.
//

use std.test as test
class T {
    n: i64
    init(n: i64) { this.n = n }
}

let baseline = test.liveAllocBytes()

let i: i64 = 0
while i < 1000 {
    let t = new T(i)
    let _ = t
    i = i + 1
}

let after = test.liveAllocBytes()
let grew = after - baseline

// Loose threshold accommodating the current object-cell leak
// (24 KB for 1000 × 24-byte instances). Tighten to e.g. < 256
// when `release_object` starts calling `host_mir_free`.
test.expectTrue(grew < 32768)
