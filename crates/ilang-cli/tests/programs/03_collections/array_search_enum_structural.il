// Array `indexOf` / `includes` / `remove` must compare enum elements
// STRUCTURALLY (discriminant + payload), matching `==` on enums (round
// 105). A payload variant like `circle(5)` is freshly boxed at each
// construction, so the needle never matched a stored element by pointer
// — `arr.includes(Shape.circle(5))` returned false and
// `arr.indexOf(...)` returned -1 even when present. Only interned unit
// variants (`dot`) happened to match. The runtime cell comparison now
// routes enum elements through `__enum_structural_eq`.
//
// expect: ok
use std.test as test
enum Shape {
    dot
    circle: (i64)
    named: (string)
    pair: (i64, string)
}

let arr: Shape[] = [
    Shape.circle(5),
    Shape.dot,
    Shape.named("hi"),
    Shape.circle(9),
    Shape.pair(1, "a"),
]

// includes: structural match on payload variants
test.expectTrue(arr.includes(Shape.circle(5)))
test.expectTrue(!arr.includes(Shape.circle(7)))
test.expectTrue(arr.includes(Shape.named("hi")))
test.expectTrue(!arr.includes(Shape.named("yo")))
test.expectTrue(arr.includes(Shape.pair(1, "a")))
test.expectTrue(!arr.includes(Shape.pair(1, "b")))   // payload differs in 2nd slot
test.expectTrue(arr.includes(Shape.dot))             // unit variant still works

// indexOf: position of the first structural match
test.expect(arr.indexOf(Shape.circle(5)), 0)
test.expect(arr.indexOf(Shape.dot), 1)
test.expect(arr.indexOf(Shape.named("hi")), 2)
test.expect(arr.indexOf(Shape.circle(9)), 3)
test.expect(arr.indexOf(Shape.pair(1, "a")), 4)
test.expect(arr.indexOf(Shape.circle(99)), -1)

// remove: drops the first structural match, shifting the rest
let arr2: Shape[] = [Shape.named("x"), Shape.dot, Shape.named("x")]
arr2.remove(Shape.named("x"))
test.expect(arr2.length, 2)
test.expect(arr2.indexOf(Shape.dot), 0)              // dot shifted to front
test.expectTrue(arr2.includes(Shape.named("x")))     // the second copy remains

// ARC: a fresh enum needle built per call must not leak (the runtime
// only borrows it for comparison)
let arr3: Shape[] = [Shape.named("k"), Shape.dot]
let acc: i64[] = [0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    if arr3.includes(Shape.named("k" + i.toString())) { acc[0] = acc[0] + 1 }
    acc[0] = acc[0] + arr3.indexOf(Shape.named("k"))
    i = i + 1
}
test.expect(test.liveAllocBytes() - d0, 0)
test.expect(acc[0], 0)                                // all includes miss (idx 0 summed)

console.log("ok")
