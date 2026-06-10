// `for x in arr` reads the length every lap (live semantics, like
// JS `for..of`): a pop inside the body ends the loop early, and a
// push visits the appended elements. The old lowering hoisted
// ArrayLen out of the loop — a pop then tripped the bounds panic
// on the stale length ("index out of bounds" instead of a graceful
// end). ArrayLoad re-reads the data pointer per access, so a
// push-triggered realloc was already safe.

use std.test as test

class Box {
    n: i64
    init(x: i64) { this.n = x }
}

// pop during iteration — early end, no bounds panic
let boxes: Box[] = [new Box(1), new Box(2), new Box(3)]
let sum = 0
for b in boxes {
    sum = sum + b.n
    if let some(_) = boxes.pop() {}
}
test.expect(sum, 3)        // visits 1 and 2; len catches up
test.expect(boxes.length, 1)

// push during iteration — appended elements are visited
let xs: i64[] = [1, 2, 3]
let total = 0
for x in xs {
    total = total + x
    if x == 1 { xs.push(99) }
}
test.expect(total, 105)    // 1 + 2 + 3 + 99
test.expect(xs.length, 4)

// clear during iteration — single visit
let ys: i64[] = [7, 8, 9]
let visits = 0
for y in ys {
    visits = visits + 1
    while ys.length > 0 {
        if let some(_) = ys.pop() {}
    }
}
test.expect(visits, 1)

// leak balance with object elements under mutation
let base = test.liveAllocBytes()
let i = 0
while i < 200 {
    let zs: Box[] = [new Box(1), new Box(2)]
    let t = 0
    for z in zs {
        t = t + z.n
        if let some(_) = zs.pop() {}
    }
    test.expect(t, 1)
    i = i + 1
}
test.expectTrue(test.liveAllocBytes() - base < 1024)
