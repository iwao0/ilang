// A capturing closure passed to a user-defined higher-order function.
// The closure runs inside the callee, and because it mutates a shared
// outer-scope binding the effect is visible after the call returns
// (shared mutable capture across the call boundary). Distinct from
// fn_ref_as_arg_and_local (top-level fn refs, no captures) and
// closure_mutates_captured_class (mutating a captured object, not an
// outer local).
use std.test as test

fn runN(f: fn(), n: i64) {
    let i = 0
    while i < n {
        f()
        i = i + 1
    }
}

fn collectLocal(): i64 {
    let total = 0
    let add3 = fn() { total = total + 3 }
    runN(add3, 4)
    total                       // closure's writes reached the outer local
}
test.expect(collectLocal(), 12)

// Same across a top-level binding (shared module slot).
let acc = 0
let addAcc = fn() { acc = acc + 7 }
runN(addAcc, 3)
test.expect(acc, 21)

// A returning higher-order fn: apply a capturing transform.
fn applyTo(f: fn(i64): i64, x: i64): i64 { f(x) }
let bias = 1000
let shift = fn(x: i64): i64 { x + bias }
test.expect(applyTo(shift, 5), 1005)
