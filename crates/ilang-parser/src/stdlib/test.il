// Built-in `test` module — assertion helpers for self-checking
// scripts and integration-test fixtures. On failure each helper prints
// a short report to stderr and aborts with non-zero exit, so a
// fixture file `use test` + a sequence of `test.expect(...)` calls
// can be the entire test program.

@extern fn expect(actual: i64, expected: i64)
@extern fn expectStr(actual: string, expected: string)
@extern fn expectBool(actual: bool, expected: bool)
@extern fn expectF64(actual: f64, expected: f64)
// Single-condition shorthand. `test.expectTrue(x < y)`.
@extern fn expectTrue(condition: bool)
@extern fn expectFalse(condition: bool)
// Always fails — useful as an explicit "should never reach" marker.
@extern fn fail(message: string)
