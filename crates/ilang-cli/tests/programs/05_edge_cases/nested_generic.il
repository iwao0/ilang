// Deeply nested generic class instantiation. Each (Box, args)
// pair monomorphizes to its own concrete class; nesting works
// without depth limits.
use std.test

class Box<T> {
    x: T
    init(v: T) { this.x = v }
    get(): T { x }
}

let b1 = new Box<i64>(7)
let b2 = new Box<Box<i64>>(b1)
let b3 = new Box<Box<Box<i64>>>(b2)
let b4 = new Box<Box<Box<Box<i64>>>>(b3)

test.expect(b4.get().get().get().get(), 7)

// Pair of two different Box instantiations.
class Pair<A, B> {
    a: A
    b: B
    init(x: A, y: B) { this.a = x; this.b = y }
}

let p = new Pair<Box<i64>, Box<string>>(new Box<i64>(42), new Box<string>("yo"))
test.expect(p.a.get(), 42)
test.expectStr(p.b.get(), "yo")
