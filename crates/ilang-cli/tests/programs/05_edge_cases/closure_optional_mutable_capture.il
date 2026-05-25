// Mutable closure capture whose static type is `T?`. Each call
// reads the current Optional via `if let some(...)`, derives the
// next value, and reassigns the captured slot to a fresh `some(...)`.
// Stresses both the cell-write of an Optional (heap-shaped for
// boxed payloads, two-state for primitives) and the discriminant
// preservation across writes.
use std.test as test
fn makeCounter(): fn(): i64 {
    let cur: i64? = none
    fn(): i64 {
        let result = if let some(v) = cur { v } else { 0 }
        cur = some(result + 1)
        result
    }
}

let f = makeCounter()
test.expect(f(), 0)
test.expect(f(), 1)
test.expect(f(), 2)
test.expect(f(), 3)

// Independent makers.
let g = makeCounter()
test.expect(g(), 0)
test.expect(f(), 4)         // f's own cell unaffected by g
test.expect(g(), 1)
