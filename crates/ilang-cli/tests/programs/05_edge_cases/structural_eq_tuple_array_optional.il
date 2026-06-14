// `==` / `!=` on heap value containers — tuples, dynamic arrays, and
// optionals — compare STRUCTURALLY (element-wise), the same value
// equality string and enum `==` already have. Before, the checker
// rejected `==` on these types entirely ("cannot apply binary op").
// Value-kind slots (numbers, strings, nested containers, enums) compare
// by value; reference-kind slots (objects) compare by pointer, matching
// `==` on classes.
//
// expect: ok
use std.test as test
let deinits: i64[] = [0]
class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

// --- tuples ---
test.expectTrue((1, 2) == (1, 2))
test.expectTrue(!((1, 2) == (1, 3)))
test.expectTrue((1, 2) != (1, 3))
test.expectTrue(("a", 1) == ("a", 1))          // string slot structural
test.expectTrue(!(("a", 1) == ("b", 1)))
test.expectTrue(((1, 2), 3) == ((1, 2), 3))    // nested tuple recurses
test.expectTrue(!(((1, 2), 3) == ((1, 9), 3)))

// --- dynamic arrays ---
let a: i64[] = [1, 2, 3]
let b: i64[] = [1, 2, 3]
let c: i64[] = [1, 2, 4]
let d: i64[] = [1, 2]
test.expectTrue(a == b)
test.expectTrue(!(a == c))
test.expectTrue(a != c)
test.expectTrue(!(a == d))                     // different length
let sa: string[] = ["x", "y"]
let sb: string[] = ["x", "y"]
test.expectTrue(sa == sb)                       // string elements structural

// --- optionals ---
let o1: i64? = some(5)
let o2: i64? = some(5)
let o3: i64? = some(6)
let o4: i64? = none
test.expectTrue(o1 == o2)
test.expectTrue(!(o1 == o3))
test.expectTrue(!(o1 == o4))                    // some vs none
test.expectTrue(o4 == none)                     // none vs none
test.expectTrue(some(7) == some(7))

// --- object slots compare by reference (consistent with class `==`) ---
let p = new Box(1)
let q = new Box(1)
test.expectTrue((p, 1) == (p, 1))               // same ref
test.expectTrue(!((p, 1) == (q, 1)))            // value-equal but different ref

// --- ARC: comparing fresh containers borrows and never leaks ---
fn cmpChurn(): i64 {
    let t = (1, "k", 2) == (1, "k", 2)
    let ar: i64[] = [1, 2, 3]
    let a2 = ar == [1, 2, 3]
    let op: i64? = some(9)
    let o = op == some(9)
    (if t { 1 } else { 0 }) + (if a2 { 1 } else { 0 }) + (if o { 1 } else { 0 })
}
let acc: i64[] = [0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 { acc[0] = acc[0] + cmpChurn(); i = i + 1 }
test.expect(acc[0], 300)
test.expect(test.liveAllocBytes() - d0, 0)

console.log("ok")
