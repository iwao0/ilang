// Overloads keyed on fn-typed parameters with distinct signatures.
// The call site routes by the closure's static fn type — passing
// `fn(i64): i64` lands on the i64-shaped overload, `fn(string):
// string` on the string one. Per syntax.md §6 + "First-class
// functions" (L538).
use std.test as test

fn run(f: fn(i64): i64): i64 { f(10) }
fn run(f: fn(string): string): string { f("hi") }

let double = fn(n: i64): i64 { n * 2 }
let bang = fn(s: string): string { s + "!" }

test.expect(run(double), 20)
test.expectStr(run(bang), "hi!")

// Anonymous closures at the call site — type drives the pick.
test.expect(run(fn(n: i64): i64 { n + 1 }), 11)
test.expectStr(run(fn(s: string): string { s + s }), "hihi")

// Top-level fn passed as a value uses the same dispatch path.
fn neg(n: i64): i64 { -n }
test.expect(run(neg), -10)

// Different arity also distinguishes overloads.
fn run2(f: fn(i64, i64): i64): i64 { f(3, 4) }
test.expect(run2(fn(a: i64, b: i64): i64 { a + b }), 7)
