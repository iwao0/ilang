// The unit type `()` is writable in type position (e.g. an explicit
// void return annotation), and a call can target an arbitrary callee
// expression — `arr[0]()`, `(f)(x)`, `mk()()` — not just a bare name.
use std.test as test

// Explicit `(): ()` void-return annotation on a fn-typed binding and a
// fn literal.
let log: fn(i32): () = fn(x: i32): () { }
log(5)

let sum: i64[] = []
let nums: i32[] = [1, 2, 3]
nums.forEach(fn(x: i32): () { sum.push(x as i64) })
test.expect(sum.length, 3)
test.expect(sum[2], 3)

// Call the result of indexing an array of closures.
let fns: (fn(): i64)[] = [fn(): i64 { 42 }, fn(): i64 { 99 }]
test.expect(fns[0](), 42)
test.expect(fns[1](), 99)

// Call the result of a call (a function returning a closure).
let mk = fn(): fn(i64): i64 { fn(x: i64): i64 { x + 1 } }
test.expect(mk()(41), 42)
