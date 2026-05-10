//! Runtime support library linked into ilang AOT executables.
//!
//! Today this is a placeholder. The next step moves the `host_*`
//! functions and table-population state out of `ilang-mir-codegen`
//! into here so that:
//! 1. JIT mode keeps using them via the rlib facet (function pointers
//!    registered with `JITBuilder::symbol`).
//! 2. AOT mode links against the `staticlib` facet so generated `.o`
//!    files can resolve `__mir_alloc` etc. at the system linker step.

#[unsafe(no_mangle)]
pub extern "C" fn __ilang_runtime_probe() -> i64 {
    0
}
