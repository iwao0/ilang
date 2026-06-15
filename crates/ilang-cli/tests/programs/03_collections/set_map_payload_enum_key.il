// A payload-carrying enum is a first-class `Set` element / `Map` key:
// dedup and lookup go through the enum's structural equality / hash
// (round 129), and the object-keyed store retains / releases the enum
// keys by KIND_ENUM (not KIND_OBJECT — their rc lives at a different
// offset). Unit / @flags enums keep the i64-tag store.
//
// Before: `new Set<PayloadEnum>()` was rejected ("must declare equals /
// hashCode") because the language offered no enum equals / hashCode.
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

// --- Set dedup by structural equality ---
let s = new Set<Shape>()
s.add(Shape.circle(5))
s.add(Shape.circle(5))        // structural duplicate
s.add(Shape.circle(9))
s.add(Shape.dot)
s.add(Shape.dot)              // duplicate (interned unit)
s.add(Shape.named("hi"))
s.add(Shape.named("hi"))      // structural duplicate
test.expect(s.size(), 4)
test.expectTrue(s.has(Shape.circle(5)))
test.expectTrue(!s.has(Shape.circle(7)))
test.expectTrue(s.has(Shape.named("hi")))
test.expectTrue(!s.has(Shape.named("yo")))
s.delete(Shape.circle(5))
test.expect(s.size(), 3)
test.expectTrue(!s.has(Shape.circle(5)))

// --- Map keyed by a payload enum ---
let m = new Map<Shape, i64>()
m[Shape.circle(1)] = 10
m[Shape.circle(1)] = 20       // same key overwrites
m[Shape.named("k")] = 30
test.expect(m[Shape.circle(1)], 20)
test.expect(m[Shape.named("k")], 30)
test.expect(m.size(), 2)
test.expectTrue(m.has(Shape.named("k")))
test.expectTrue(!m.has(Shape.named("x")))

// --- iteration: forEach over an enum-element set ---
fn weight(sh: Shape): i64 {
    match sh {
        circle(n) { n }
        named(t) { t.length }
        dot { return 0 }
    }
}
let s2 = new Set<Shape>()
s2.add(Shape.circle(3))
s2.add(Shape.named("ab"))
let total = 0
s2.forEach(fn(sh: Shape) { total = total + weight(sh) })
test.expect(total, 5)         // 3 + 2

// map.keys() over enum keys
let m2 = new Map<Shape, i64>()
m2[Shape.circle(4)] = 0
m2[Shape.named("xyz")] = 0
let ksum = 0
for k in m2.keys() { ksum = ksum + weight(k) }
test.expect(ksum, 7)          // 4 + 3

// --- ARC: an enum key carrying a heap payload is released by kind ---
enum Holder {
    none2
    box: (Box)
}
// Warm up the `none2` unit-variant intern (a one-time singleton alloc)
// so it doesn't land in the measurement window below.
{
    let warm = new Set<Holder>()
    warm.add(Holder.none2)
}
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    let hs = new Set<Holder>()
    hs.add(Holder.box(new Box(i)))   // enum wraps a Box; set owns the key
    hs.add(Holder.none2)
    i = i + 1
    // hs drops here -> releasing the enum key cascades into the Box
}
test.expect(deinits[0] - bd, 100)    // one Box made + freed per iteration
test.expect(test.liveAllocBytes() - d0, 0)

console.log("ok")
