// `if` / `match` branches whose arms are generic-class instantiations
// join through the arms' BASE class, so they take the common interface
// just like non-generic conformers do. Previously the branch-join only
// handled plain `Type::Object` arms, so a `Box<i64>` arm (a
// `Type::Generic`) was rejected against an `Empty` or `Box<string>` arm
// ("expected Box<i64>, got Empty"), even with a `Sized` annotation.
// Folding 3+ arms also needed `common_object_join` to combine an already-
// computed interface (`Sized`) with a further implementer.
//
// expect: ok
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

// if-join of a generic instance with a non-generic one.
fn pick(n: i64): Sized {
    if n == 0 { new Box<i64>(1) } else { new Empty() }
}
test.expect(pick(0).size(), 1)
test.expect(pick(1).size(), 0)

// if-join of two DIFFERENT generic instantiations.
let c = true
let s: Sized = if c { new Box<i64>(1) } else { new Pair<i64, string>(1, "x") }
test.expect(s.size(), 1)

// match-join folding generic + generic + non-generic arms.
let m: Sized = match 2 {
    0 { new Box<i64>(9) }
    1 { new Pair<i64, i64>(1, 2) }
    _ { new Empty() }
}
test.expect(m.size(), 0)

console.log("ok")
