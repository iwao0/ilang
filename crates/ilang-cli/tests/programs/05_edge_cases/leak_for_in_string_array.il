// `for s in fresh_string_array() { ... }` — the iter is a fresh
// `string[]` whose elements are rc-tracked StringRc entries. The
// for-in path has to release the iter array (cell + buffer) at
// loop exit AND ensure each per-iteration `s` binding's StringRc
// drops at the body's scope exit. Combines `leak_for_in_fresh
// _receiver.il` (array reclaim) with the string-element cascade.
//

use std.test as test
fn makeStrs(n: i64): string[] {
    let xs: string[] = []
    let i: i64 = 0
    while i < n {
        xs.push("item")
        i = i + 1
    }
    xs
}

let baseline = test.liveAllocBytes()
let baseline_s = test.liveStringCount()

let i: i64 = 0
while i < 100 {
    for s in makeStrs(5) {
        let _ = s.length
    }
    i = i + 1
}

let after = test.liveAllocBytes()
let after_s = test.liveStringCount()
test.expectTrue(after - baseline < 65536)
test.expectTrue(after_s - baseline_s < 50)
