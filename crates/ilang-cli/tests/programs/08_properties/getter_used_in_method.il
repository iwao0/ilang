// A method can read the class's own `get` property through `this`
// — the property dispatch resolves inside the class body just
// like a field read.
use std.test as test

class Box {
    n: i64
    init(n: i64) { this.n = n }
    get value(): i64 { this.n }
    get doubled(): i64 { this.value * 2 }
    describe(): string {
        // Same-class getter reached through `this`. Stitches
        // string + i64 to also exercise template-style append.
        "value=" + this.value.toString()
    }
}

let b = new Box(7)
test.expect(b.value, 7)
test.expect(b.doubled, 14)
test.expectStr(b.describe(), "value=7")
