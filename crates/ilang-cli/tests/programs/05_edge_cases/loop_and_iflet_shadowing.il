// A for-loop variable, an inner block's `let`, and `if let some(v)`
// each introduce a fresh binding scoped to the loop / block / arm
// that shadows but never overwrites a same-named outer binding.
use std.test

// for-in loop variable doesn't leak out (even with the same name).
let i = 100
for i in 0..5 { }
test.expect(i, 100)

// Inner block's `let` shadows outer.
let x = 10
{
    let x = 20
    test.expect(x, 20)
    {
        let x = 30
        test.expect(x, 30)
    }
    test.expect(x, 20)        // mid-block restoration
}
test.expect(x, 10)            // outer restored

// `if let some(name) = ...` introduces `name` only inside `then`.
let v: i64? = some(7)
let z = 99
if let some(z) = v {
    test.expect(z, 7)         // pattern binding
}
test.expect(z, 99)            // outer unchanged

// Same shadowing with the `else` branch reading the outer.
let outer = 42
let absent: i64? = none
if let some(outer) = absent {
    test.expect(outer, 0)     // unreachable
} else {
    test.expect(outer, 42)    // outer remains
}
test.expect(outer, 42)
