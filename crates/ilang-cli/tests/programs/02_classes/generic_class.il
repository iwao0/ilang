// Generic classes — must be instantiated with explicit type args.
// Each (Name, args) combo is monomorphized to a distinct concrete
// class in the JIT.
use test

class Box<T> {
    x: T
    init(v: T) { this.x = v }
    get(): T { x }
}

class Pair<A, B> {
    a: A
    b: B
    init(x: A, y: B) { this.a = x; this.b = y }
}

let bi = new Box<i64>(42)
let bs = new Box<string>("hi")
test.expect(bi.get(), 42)
test.expectStr(bs.get(), "hi")

let p = new Pair<string, i64>("k", 1)
test.expectStr(p.a, "k")
test.expect(p.b, 1)

// Nested generic: Box<Box<i64>>.
let nested = new Box<Box<i64>>(new Box<i64>(99))
test.expect(nested.get().get(), 99)

// Field reassignment with generic type.
let bi2 = new Box<i64>(1)
bi2.x = 100
test.expect(bi2.get(), 100)
