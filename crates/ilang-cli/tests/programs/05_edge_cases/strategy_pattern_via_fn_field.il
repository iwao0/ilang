// Strategy-pattern shape: a class stores a `fn(...)` field and
// dispatches through it. The field can be replaced at runtime to
// swap behaviour. Verifies that the closure value survives storage
// in a field, retrieval via `let`, and replacement (which must
// release the previous closure).
use std.test as test
class Strategy {
    f: fn(i64): i64
    init(g: fn(i64): i64) { this.f = g }
    apply(x: i64): i64 {
        let g = this.f
        g(x)
    }
}

let square = fn(x: i64): i64 { x * x }
let s = new Strategy(square)
test.expect(s.apply(5), 25)
test.expect(s.apply(-3), 9)

// Replace with another strategy at runtime.
s.f = fn(x: i64): i64 { x + 100 }
test.expect(s.apply(5), 105)
test.expect(s.apply(0), 100)

// Capturing closure as the strategy.
let factor = 7
s.f = fn(x: i64): i64 { x * factor }
test.expect(s.apply(3), 21)

// Outer rebind doesn't affect the captured snapshot.
let factor = 999
test.expect(s.apply(3), 21)
