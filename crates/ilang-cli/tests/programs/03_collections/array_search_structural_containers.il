// Array `indexOf` / `includes` / `remove` compare element STRUCTURALLY
// for tuple, nested-dynamic-array, and optional element types (the same
// way round 126 did for enums). Before, these element kinds fell through
// to pointer comparison in the runtime cell match, so a freshly-built
// needle never matched a stored element — `[(1,2)].includes((1,2))`
// returned false. Now they route through `value_structural_eq`, aligned
// with the `==` operator on the same types.
//
// expect: ok
use std.test as test

// --- array of tuples ---
let ts: (i64, i64)[] = [(1, 2), (3, 4), (5, 6)]
test.expectTrue(ts.includes((3, 4)))
test.expectTrue(!ts.includes((3, 9)))
test.expect(ts.indexOf((5, 6)), 2)
test.expect(ts.indexOf((9, 9)), -1)

// tuples with a string slot
let ss: (string, i64)[] = [("a", 1), ("b", 2)]
test.expectTrue(ss.includes(("b", 2)))
test.expectTrue(!ss.includes(("b", 3)))

// --- array of dynamic arrays ---
let aa: (i64[])[] = [[1, 2], [3, 4]]
test.expectTrue(aa.includes([1, 2]))
test.expectTrue(!aa.includes([1, 3]))
test.expect(aa.indexOf([3, 4]), 1)

// --- array of optionals ---
let os: (i64?)[] = [some(1), none, some(3)]
test.expectTrue(os.includes(some(1)))
test.expectTrue(os.includes(none))
test.expectTrue(!os.includes(some(2)))
test.expect(os.indexOf(some(3)), 2)
test.expect(os.indexOf(none), 1)

// --- remove drops the first structural match ---
let rm: (i64, i64)[] = [(1, 1), (2, 2), (1, 1)]
rm.remove((1, 1))
test.expect(rm.length, 2)
test.expect(rm.indexOf((2, 2)), 0)
test.expectTrue(rm.includes((1, 1)))             // the second copy remains

// --- ARC: a fresh container needle is only borrowed, no leak ---
let probe: (i64, i64)[] = [(7, 8)]
let acc: i64[] = [0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    if probe.includes((7, i)) { acc[0] = acc[0] + 1 }    // miss except i==8
    acc[0] = acc[0] + probe.indexOf((7, 8))              // hit -> 0
    i = i + 1
}
test.expect(test.liveAllocBytes() - d0, 0)
test.expect(acc[0], 1)                                    // only i==8 matches

console.log("ok")
