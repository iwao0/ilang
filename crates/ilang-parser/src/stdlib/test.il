// Built-in `test` module — assertion helpers for self-checking
// scripts and integration-test fixtures. On failure each helper prints
// a short report to stderr and aborts with non-zero exit, so a
// fixture file `use test` + a sequence of `test.expect(...)` calls
// can be the entire test program.

@extern(C) {
    /// Asserts `actual == expected` for i64 values. On failure prints
    /// both values to stderr and aborts the process.
    pub fn expect(actual: i64, expected: i64)
    /// Asserts `actual == expected` for string values. On failure
    /// prints both strings to stderr and aborts.
    pub fn expectStr(actual: string, expected: string)
    /// Asserts `actual == expected` for bool values. On failure prints
    /// both values to stderr and aborts.
    pub fn expectBool(actual: bool, expected: bool)
    /// Asserts `actual == expected` for f64 values (bit-exact compare,
    /// no epsilon). On failure prints both values to stderr and aborts.
    pub fn expectF64(actual: f64, expected: f64)
    /// Single-condition shorthand. `test.expectTrue(x < y)` aborts
    /// unless `condition` is true.
    pub fn expectTrue(condition: bool)
    /// Single-condition shorthand. `test.expectFalse(x < y)` aborts
    /// unless `condition` is false.
    pub fn expectFalse(condition: bool)
    /// Always fails — useful as an explicit "should never reach"
    /// marker. Prints `message` to stderr and aborts.
    pub fn fail(message: string)
    /// Calls a 2-arg i32-returning fn pointer with the given i64 args
    /// and returns the result. Lets the test suite verify that ilang
    /// fns can be invoked from non-Cranelift code (simulating arbitrary
    /// C callers). JIT-only — the interpreter handler reports
    /// "no extern handler".
    pub fn applyI32Cb(cb: fn(i64, i64): i32, a: i64, b: i64): i32
    /// Counter-wrapped libc::free. Calling this counts toward
    /// `countedFreeCount`. Used by FFI tests to confirm a custom
    /// free fn fires after a strdup-like copy. JIT-only — the
    /// interpreter has no native free pipeline.
    pub fn countedFree(ptr: i64)
    /// Number of times `countedFree` has been called in this process.
    pub fn countedFreeCount(): i32
    /// Currently-held bytes through the mir-jit `host_mir_alloc`
    /// tracker (Object / Array / Optional / Tuple / Closure / internal
    /// cells). Used by leak-detection fixtures to take a baseline
    /// before a workload and assert no growth afterward. mir-jit only
    /// — the legacy backend doesn't share this allocator tracking.
    pub fn liveAllocBytes(): i64
    /// Currently-held alloc count through the mir-jit `host_mir_alloc`
    /// tracker. mir-jit only — see `liveAllocBytes` for the same
    /// caveat.
    pub fn liveAllocCount(): i64
    /// Number of entries currently in the rc-tracked string registry
    /// (leak_cstring buffers). Used by leak-detection fixtures for
    /// string-producing operations. mir-jit only.
    pub fn liveStringCount(): i64
}
