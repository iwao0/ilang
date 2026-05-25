// Function overloading by parameter type. Same name, different
// implementations, dispatched at the call site by exact match.
use std.test as test
fn show(n: i64): string { "int" }
fn show(s: string): string { "str" }
fn show(b: bool): string { "bool" }
fn show(x: f64): string { "float" }

test.expectStr(show(42), "int")
test.expectStr(show("hi"), "str")
test.expectStr(show(true), "bool")
test.expectStr(show(3.14), "float")
