// `Map<K, ClassT>` — values are heap pointers. Index-write replaces
// the previous instance (which must be released so its deinit fires
// only when no other binding holds it); reads return the current
// instance pointer.
use test

class Point {
    x: i64
    y: i64
    init(a: i64, b: i64) { this.x = a; this.y = b }
}

let m = new Map<string, Point>()
m["a"] = new Point(1, 2)
m["b"] = new Point(3, 4)

test.expect(m["a"].x, 1)
test.expect(m["a"].y, 2)
test.expect(m["b"].x, 3)
test.expect(m.size(), 2)

// Replace `a` — old instance released; new one observable.
m["a"] = new Point(10, 20)
test.expect(m["a"].x, 10)
test.expect(m["a"].y, 20)
test.expect(m.size(), 2)

// Iterate values and sum.
let total = 0
for p in m.values() {
    total += p.x + p.y
}
test.expect(total, 37)             // 10+20 + 3+4

// Delete preserves remaining values.
m.delete("a")
test.expect(m.size(), 1)
test.expect(m["b"].x, 3)
let after_total = 0
for p in m.values() { after_total += p.x + p.y }
test.expect(after_total, 7)
