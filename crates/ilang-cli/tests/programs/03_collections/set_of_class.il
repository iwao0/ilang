// Set<MyClass> driven by the value-equality protocol (manual
// `equals` + `hashCode` methods). Covers the full Set method
// matrix on a class-typed element so the Object store handles
// add / has / delete / size / clear / values / forEach plus
// union / intersection / difference / isSubsetOf / isSupersetOf /
// isDisjointFrom exactly like the primitive stores.
use std.test as test

class Point {
    pub x: i64
    pub y: i64
    pub init(x: i64, y: i64) {
        this.x = x
        this.y = y
    }
    pub equals(other: Point): bool {
        this.x == other.x && this.y == other.y
    }
    pub hashCode(): i64 {
        this.x * 31 + this.y
    }
}

// ---- basics: add / dedup / has / delete / size ----
let s = new Set<Point>()
test.expect(s.size(), 0)
s.add(new Point(1, 2))
s.add(new Point(3, 4))
s.add(new Point(1, 2))   // dup under value-equality — collapses
test.expect(s.size(), 2)
test.expectBool(s.has(new Point(1, 2)), true)
test.expectBool(s.has(new Point(9, 9)), false)
test.expectBool(s.delete(new Point(3, 4)), true)
test.expectBool(s.delete(new Point(3, 4)), false)
test.expect(s.size(), 1)

// ---- values() + forEach ----
let v = new Set<Point>()
v.add(new Point(10, 20))
v.add(new Point(30, 40))
let xs = v.values()
test.expect(xs.length, 2)
let sum = 0
v.forEach(fn(p: Point) { sum = sum + p.x + p.y })
test.expect(sum, 100)   // 10+20 + 30+40

// ---- clear ----
v.clear()
test.expect(v.size(), 0)

// ---- set algebra: union / intersection / difference ----
let a = new Set<Point>()
a.add(new Point(1, 1))
a.add(new Point(2, 2))
a.add(new Point(3, 3))
let b = new Set<Point>()
b.add(new Point(2, 2))
b.add(new Point(3, 3))
b.add(new Point(4, 4))
test.expect(a.union(b).size(), 4)
test.expect(a.intersection(b).size(), 2)
test.expect(a.difference(b).size(), 1)
test.expectBool(a.union(b).has(new Point(1, 1)), true)
test.expectBool(a.union(b).has(new Point(4, 4)), true)
test.expectBool(a.intersection(b).has(new Point(2, 2)), true)
test.expectBool(a.intersection(b).has(new Point(1, 1)), false)
test.expectBool(a.difference(b).has(new Point(1, 1)), true)
test.expectBool(a.difference(b).has(new Point(2, 2)), false)

// ---- predicates: isSubsetOf / isSupersetOf / isDisjointFrom ----
test.expectBool(a.isSubsetOf(a.union(b)), true)
test.expectBool(a.isSubsetOf(b), false)
test.expectBool(a.union(b).isSupersetOf(a), true)
test.expectBool(a.isSupersetOf(b), false)
test.expectBool(a.isDisjointFrom(b), false)
let z = new Set<Point>()
z.add(new Point(99, 99))
test.expectBool(a.isDisjointFrom(z), true)
test.expectBool(z.isDisjointFrom(a), true)
