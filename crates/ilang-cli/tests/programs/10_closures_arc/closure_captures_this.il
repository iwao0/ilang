// Closure built inside a method body captures the lexical `this`
// (e416943 + 9475956: interpreter and JIT both restore it; the
// codegen path also retains/releases the captured pointer to
// avoid premature drop on explicit return — 0f5fdc4).
use std.test

class Foo {
    x: i64
    init() { this.x = 0 }
    make(): fn(): i64 {
        fn(): i64 { this.x }
    }
    make_explicit(): fn(): i64 {
        fn(): i64 { return this.x }
    }
}

let f = new Foo()
f.x = 42
let g = f.make()
let h = f.make_explicit()
for _ in 0..200 {
    test.expect(g(), 42)
    test.expect(h(), 42)
}
f.x = 99
test.expect(g(), 99)
test.expect(h(), 99)
