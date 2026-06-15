// Clean-round pin for payload-enum Set / Map keys (round 130): the set
// ALGEBRA (union / intersection / difference), the subset / superset /
// disjoint predicates, and Map `entries()` all work on payload-enum
// elements, and the operations that build a result set retain / release
// the enum keys by kind (no leak, no corruption). Round 130's fixture
// covered add / has / delete / forEach / keys but not these.
//
// expect: ok
use std.test as test
let deinits: i64[] = [0]
class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

enum Shape {
    dot
    circle: (i64)
    named: (string)
}

let a = new Set<Shape>()
a.add(Shape.circle(1))
a.add(Shape.circle(2))
a.add(Shape.dot)
let b = new Set<Shape>()
b.add(Shape.circle(2))
b.add(Shape.named("x"))

// --- set algebra ---
let u = a.union(b)
test.expect(u.size(), 4)                 // circle(1), circle(2), dot, named(x)
test.expectTrue(u.has(Shape.named("x")))
let inter = a.intersection(b)
test.expect(inter.size(), 1)             // circle(2)
test.expectTrue(inter.has(Shape.circle(2)))
test.expectTrue(!inter.has(Shape.circle(1)))
let diff = a.difference(b)
test.expect(diff.size(), 2)              // circle(1), dot
test.expectTrue(diff.has(Shape.circle(1)))
test.expectTrue(!diff.has(Shape.circle(2)))

// --- predicates ---
let c = new Set<Shape>()
c.add(Shape.circle(1))
test.expectTrue(c.isSubsetOf(a))
test.expectTrue(a.isSupersetOf(c))
let e = new Set<Shape>()
e.add(Shape.named("z"))
test.expectTrue(e.isDisjointFrom(a))
test.expectTrue(!c.isDisjointFrom(a))

// --- Map entries() over enum keys ---
let m = new Map<Shape, i64>()
m[Shape.circle(7)] = 70
m[Shape.named("k")] = 30
let vsum = 0
for ent in m.entries() {
    vsum = vsum + ent[1]
}
test.expect(vsum, 100)

// --- ARC: union of heap-payload-enum sets retains keys by kind ---
enum H {
    box: (Box)
    tag: (string)
}
// warm the `tag` registry / interns out of the measurement window
{
    let w = new Set<H>()
    w.add(H.tag("z"))
}
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    let s1 = new Set<H>()
    s1.add(H.box(new Box(i)))
    s1.add(H.tag("z"))
    let s2 = new Set<H>()
    s2.add(H.box(new Box(i + 1000)))
    let _u = s1.union(s2)            // result shares element refs; all drop here
    i = i + 1
}
test.expect(deinits[0] - bd, 200)   // Box(i) + Box(i+1000) per iteration
test.expect(test.liveAllocBytes() - d0, 0)

console.log("ok")
