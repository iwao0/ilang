// Callback-driven iteration must snapshot: the callback may mutate
// the container mid-walk. Array forEach / map / filter / find /
// findIndex / every / some read (len, data) once and then iterated
// the LIVE buffer — a push that reallocated (or a pop that shrank)
// left them reading freed memory (`arr.push` inside `arr.forEach`
// truncated the walk over a dangling pointer). They now copy the
// cells up front and hold a +1 on heap elements for the duration.
// Map forEach snapshotted entries but only retained KEYS — a
// delete / overwrite inside the callback freed a snapshot VALUE
// before its visit (reads flapped between runs). Values retain too.

use std.test as test

class Box {
    n: i64
    init(x: i64) { this.n = x }
}

// array: push during forEach — the walk sees the snapshot
let arr: i64[] = [1, 2, 3]
let total = 0
arr.forEach(fn(x: i64) {
    total = total + x
    if x == 1 { arr.push(99) }
})
test.expect(total, 6)
test.expect(arr.length, 4)

// array of objects: pop during forEach — the popped element's
// snapshot share keeps it readable
let boxes: Box[] = [new Box(1), new Box(2), new Box(3)]
let sum = 0
boxes.forEach(fn(b: Box) {
    sum = sum + b.n
    if let some(_) = boxes.pop() {}
})
test.expect(sum, 6)
test.expect(boxes.length, 0)

// map / filter / find with mutation in the callback
let src: i64[] = [1, 2, 3]
let doubled = src.map(fn(x: i64): i64 {
    src.push(x * 10)
    x * 2
})
test.expect(doubled.length, 3)
test.expect(doubled[2], 6)
test.expect(src.length, 6)

let evens = src.filter(fn(x: i64): bool {
    if let some(_) = src.pop() {}
    x % 2 == 0
})
test.expectTrue(evens.length >= 1)

// map forEach: delete inside the callback — snapshot values stay
// alive for their visit
let m = new Map<string, Box>()
m["a"] = new Box(1)
m["b"] = new Box(2)
let seen = 0
m.forEach(fn(k: string, v: Box) {
    seen = seen + v.n
    let _ = m.delete("a")
    let _ = m.delete("b")
})
test.expect(seen, 3)
test.expect(m.size(), 0)

// overwrite during forEach: the visited value is the snapshot's
let m2 = new Map<string, Box>()
m2["k"] = new Box(5)
m2.forEach(fn(k: string, v: Box) {
    m2["k"] = new Box(99)
    test.expect(v.n, 5)
})
test.expect(m2["k"].n, 99)

// leak balance across mutating iterations
let base = test.liveAllocBytes()
let i = 0
while i < 200 {
    let xs: Box[] = [new Box(1), new Box(2)]
    let t = 0
    xs.forEach(fn(b: Box) {
        t = t + b.n
        if let some(_) = xs.pop() {}
    })
    test.expect(t, 3)
    i = i + 1
}
test.expectTrue(test.liveAllocBytes() - base < 1024)
