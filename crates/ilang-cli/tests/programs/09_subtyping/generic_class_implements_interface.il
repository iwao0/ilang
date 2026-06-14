// A generic class that declares an interface (`class Box<T>: Sized`)
// should have each instantiation (`Box<i64>`) satisfy that interface —
// the relation lives on the base class, independent of the type args.
// The checker only extracted the conformance candidate from a plain
// `Type::Object`, so a `Box<i64>` value was rejected where `Sized` was
// expected ("type mismatch: expected Sized, got Box<i64>"), even though
// `box.size()` worked directly. The conformance check now also accepts a
// generic instantiation via its base class.
use std.test as test

interface Sized { size(): i64 }

class Box<T>: Sized {
    pub v: T
    init(v: T) { this.v = v }
    size(): i64 { 1 }
}

class Pair<A, B>: Sized {
    pub a: A
    pub b: B
    init(a: A, b: B) { this.a = a; this.b = b }
    size(): i64 { 2 }
}

class Empty: Sized {
    init() {}
    size(): i64 { 0 }
}

fn describe(s: Sized): i64 { s.size() }

// A generic instance flows into an interface parameter.
test.expect(describe(new Box<i64>(5)), 1)
test.expect(describe(new Box<string>("x")), 1)

// And into an interface-typed local.
let s: Sized = new Pair<i64, string>(1, "x")
test.expect(s.size(), 2)

// A heterogeneous interface array mixing generic and non-generic
// implementers dispatches dynamically.
let items: Sized[] = [
    new Box<i64>(5),
    new Pair<i64, string>(1, "x"),
    new Empty(),
]
let total = 0
let i = 0
while i < items.length {
    total = total + items[i].size()
    i = i + 1
}
test.expect(total, 3)
