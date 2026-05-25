// `for x in make_arr()` — the fresh array returned by `make_arr`
// has no surviving binding after the loop, so its data buffer
// + 48-byte header must be freed at the loop's exit sweep
// (lower_for_in's iter_is_fresh path emits the explicit
// release).
//
// `transient_for_in_and_map_method.il` already exercises this
// via deinit traces; this fixture pins down the actual memory
// reclaim using `liveAllocBytes`.
//

use std.test as test
fn make_arr(): i64[] { [1, 2, 3, 4, 5] }

let baseline = test.liveAllocBytes()

let outer: i64 = 0
while outer < 1000 {
    let sum: i64 = 0
    for x in make_arr() {
        sum = sum + x
    }
    let _ = sum
    outer = outer + 1
}

let after = test.liveAllocBytes()
let grew = after - baseline

// Each fresh array would be 48-byte header + 5*8 data = 88 bytes.
// 1000 leaks = ~88 KB. With proper release, growth is bounded
// by allocator overhead.
test.expectTrue(grew < 1024)
