/// C standard `<assert.h>` — the underlying failure handler.
///
/// The `assert()` macro itself is preprocessor-only and isn't
/// exposed here. When an assertion in a C-compiled translation unit
/// fails, control reaches one of these implementation-defined
/// functions; they're useful for crash-loop debugging and as the
/// target of intentional ilang-side aborts.
///
/// Per-platform symbol names:
///   * glibc:        `__assert_fail(expr, file, line, func)`
///   * musl:         `__assert_fail(expr, file, line, func)`
///   * macOS:        `__assert_rtn(func, file, line, expr)`
///   * msvcrt/ucrt:  `_assert(expr, file, line)`        (CRT)
///                   `_wassert(expr, file, line)`        (wide form)
///
/// All four are `@optional`; calling one on the wrong host will
/// trigger the unresolved-symbol panic by design — the function is
/// only meant to be reached when something has already gone wrong.

@extern(C, "c") {
    @lib @optional pub fn __assert_fail(
        expr: *const char,
        file: *const char,
        line: u32,
        func: *const char
    )

    @lib @optional pub fn __assert_rtn(
        func: *const char,
        file: *const char,
        line: i32,
        expr: *const char
    )

    @lib @optional pub fn _assert(
        expr: *const char,
        file: *const char,
        line: u32
    )

    @lib @optional pub fn _wassert(
        expr: *const u32,
        file: *const u32,
        line: u32
    )
}
