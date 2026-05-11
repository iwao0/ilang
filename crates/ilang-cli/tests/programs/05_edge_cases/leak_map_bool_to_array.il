// `Map<bool, T[]>` — a 2-key map where the keys are booleans and
// the values are heap-typed arrays. The map cell + value cascade
// must reclaim both arrays on map drop. `leak_map_int_to_array
// .il` covers `Map<i64, T[]>`; this fixture is the bool-key
// variant which exercises a different MapKey hashing path.
//

use test

let baseline = test.liveAllocBytes()

let i: i64 = 0
while i < 100 {
    let m = new Map<bool, i64[]>()
    m[true] = [i, i * 2]
    m[false] = [i * 3, i * 4, i * 5]
    let _ = m[true].length
    i = i + 1
}

let after = test.liveAllocBytes()
test.expectTrue(after - baseline < 65536)
