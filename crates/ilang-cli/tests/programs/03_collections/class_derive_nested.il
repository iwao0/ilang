// `@derive(Eq, Hash)` walks fields field-by-field; a class-typed
// field recurses through that field's own `equals` / `hashCode`.
// Two-level nesting verifies the recursion: `Outer` derives both
// methods, one field is an `Inner` that also derives both, and
// equality / hashing must travel through.
use std.test as test

@derive(Eq, Hash)
class Inner {
    pub a: i64
    pub b: i64
    pub init(a: i64, b: i64) {
        this.a = a
        this.b = b
    }
}

@derive(Eq, Hash)
class Outer {
    pub tag: i64
    pub inner: Inner
    pub init(tag: i64, inner: Inner) {
        this.tag = tag
        this.inner = inner
    }
}

let s = new Set<Outer>()
s.add(new Outer(1, new Inner(10, 20)))
s.add(new Outer(2, new Inner(10, 20)))
s.add(new Outer(1, new Inner(10, 20)))   // full structural dup
s.add(new Outer(1, new Inner(99, 99)))   // inner differs
test.expect(s.size(), 3)
test.expectBool(s.has(new Outer(1, new Inner(10, 20))), true)
test.expectBool(s.has(new Outer(1, new Inner(99, 99))), true)
test.expectBool(s.has(new Outer(3, new Inner(0, 0))), false)

// Map with nested-derived key
let m = new Map<Outer, string>()
m.set(new Outer(1, new Inner(10, 20)), "first")
m.set(new Outer(1, new Inner(10, 20)), "overwritten")
m.set(new Outer(2, new Inner(10, 20)), "second")
test.expect(m.size(), 2)
test.expectStr(m[new Outer(1, new Inner(10, 20))], "overwritten")
test.expectStr(m[new Outer(2, new Inner(10, 20))], "second")
