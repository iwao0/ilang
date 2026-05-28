// Immediately-invoked function expressions: a parenthesised closure
// called on the spot, `(fn() { ... })()`. Covers a bare IIFE, one that
// captures an outer binding, and one that returns a closure invoked in
// the same expression — all rely on calling an arbitrary (non-name)
// callee expression.
use std.test as test

// Bare IIFE.
let a = (fn(): i64 { 7 + 8 })()
test.expect(a, 15)

// IIFE capturing an outer binding.
let base = 100
let b = (fn(x: i64): i64 { x + base })(5)
test.expect(b, 105)

// IIFE returning a closure, invoked in one expression (chained calls).
let c = (fn(): fn(i64): i64 { fn(x: i64): i64 { x * 3 } })()(4)
test.expect(c, 12)

// IIFE used to initialise from a small computation with a local.
let d = (fn(): i64 {
    let acc = 0
    let xs: i64[] = [1, 2, 3, 4]
    xs.forEach(fn(x: i64) { acc = acc + x })
    acc
})()
test.expect(d, 10)
