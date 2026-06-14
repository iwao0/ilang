// Composing user generic free functions: one generic call's result is
// the argument to another, and both type params are solved purely by
// inference (no annotations). Existing generic-fn fixtures nest enum /
// Result constructors inside a generic call; this pins generic-fn-into-
// generic-fn, where the inner call must monomorphize first and its
// result type then drives the outer call's `T` — across distinct element
// types and a tuple-returning generic.
//
// expect: ok
use std.test as test

fn wrap<T>(x: T): T[] { [x] }
fn firstOf<T>(xs: T[]): T { xs[0] }
fn dup<T>(x: T): (T, T) { (x, x) }

// Generic result feeds directly into another generic call.
test.expect(firstOf(wrap(99)), 99)
test.expectStr(firstOf(wrap("z")), "z")

// Three-deep composition.
test.expect(firstOf(wrap(firstOf(wrap(7)))), 7)

// Generic feeding a tuple-returning generic, then destructured.
let (a, b) = dup(firstOf(wrap(5)))
test.expect(a + b, 10)

console.log("ok")
