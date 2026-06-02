// Regression: an ilang `fn(...)` value, when received as a
// function parameter and re-passed to a C `@extern(C)` function
// expecting that same callback signature, must reach the C side
// as a real function pointer (not the address of the ilang
// closure box). Direct registration (the bare `pub fn` name
// passed straight to an extern callee inside `@extern(C)`)
// already worked via the `Inst::FuncAddr` fast path; this
// fixture pins down the re-forwarding shape — a friendly ilang
// wrapper that accepts the callback and hands it off.
//
// Host helper (test_externs.rs):
//   register_and_invoke(cb) invokes cb(42, 100, 200, 300) once.
//
// expect: 642
// expect: 642
use std.test as test

@extern(C) {
    fn register_and_invoke(cb: fn(i32, i64, i64, i64): i64): i64

    pub fn myCb(a: i32, b: i64, c: i64, d: i64): i64 {
        a as i64 + b + c + d
    }
}

// Friendly wrapper that re-forwards the callback. The fix
// target: before, this lowered the local `cb` (a closure box
// pointer) directly to the C side, and C dereferenced &box
// instead of the underlying fn pointer.
fn forward(cb: fn(i32, i64, i64, i64): i64): i64 {
    register_and_invoke(cb)
}

let direct = register_and_invoke(myCb)
let via    = forward(myCb)
console.log(direct)
console.log(via)
test.expect(direct, 642)
test.expect(via, direct)
