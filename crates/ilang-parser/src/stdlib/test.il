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
// Calls a 2-arg i32-returning fn pointer with the given i64 args
// and returns the result. Lets the test suite verify that ilang
// fns can be invoked from non-Cranelift code (simulating arbitrary
// C callers). JIT-only — the interpreter handler reports
// "no extern handler".
@extern fn applyI32Cb(cb: fn(i64, i64): i32, a: i64, b: i64): i32
// Counter-wrapped libc::free. Calling this counts toward
// `countedFreeCount`. Used by `free_with` tests to confirm a
// custom free fn fires at owned_return cleanup. JIT-only — the
// interpreter has no native free pipeline.
@extern fn countedFree(ptr: i64)
@extern fn countedFreeCount(): i32
