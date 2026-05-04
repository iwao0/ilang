// `static` inside `@extern(C) { ... }` reads and writes a C global
// variable directly. Library-form (`@lib("libfoo") static …`) goes
// through dlsym on the named library; host-form (no `@lib`) uses an
// address pre-registered by the host (here
// `test_externs::register_test_static_addrs`). Reads / writes lower
// to a load / store against the resolved address — no marshalling,
// just the natural Cranelift width for the declared primitive type.
//
// JIT-only — the interpreter has no dlsym path to a real C global.
// interp: skip
use test

@extern(C) {
    static test_static_i32: i32
    static test_static_f64: f64
}

// Initial state: 0 (zero-initialized Rust static).
test.expect(test_static_i32 as i64, 0)
test.expectF64(test_static_f64, 0.0)

// Write through the global.
test_static_i32 = 42 as i32
test.expect(test_static_i32 as i64, 42)

// Same address — re-read sees the latest write.
test_static_i32 = -17 as i32
test.expect(test_static_i32 as i64, -17)

// f64 path uses an F64 load/store width.
test_static_f64 = 3.5
test.expectF64(test_static_f64, 3.5)

test_static_f64 = -2.25
test.expectF64(test_static_f64, -2.25)

// Reset to 0 so a future test run sees the same initial state.
test_static_i32 = 0 as i32
test_static_f64 = 0.0
