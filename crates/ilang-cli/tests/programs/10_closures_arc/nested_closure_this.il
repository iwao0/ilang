// Nested closure where only the innermost mentions `this` —
// every wrapper along the chain still has to capture it
// (9475956: `block_uses_this_directly` recurses into FnExpr).
use std.test

class Foo {
    x: i64
    init() { this.x = 0 }
    make(): fn(): fn(): i64 {
        fn(): fn(): i64 {
            fn(): i64 { this.x }
        }
    }
}

let f = new Foo()
f.x = 7
let outer = f.make()
let inner = outer()
test.expect(inner(), 7)
