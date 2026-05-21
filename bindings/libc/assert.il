/// C standard `<assert.h>` — assertion helpers.
///
/// C's `assert(expr)` is a preprocessor macro that captures `#expr`,
/// `__FILE__`, and `__LINE__` at the call site, then routes failure
/// through an implementation-defined symbol (`__assert_fail` on
/// glibc / musl, `__assert_rtn` on macOS, `_assert` / `_wassert` on
/// ucrt). ilang has no preprocessor, so the source-text + file +
/// line capture isn't recoverable from a plain function call.
///
/// What we expose:
///   * `assert(cond)`            — abort if `cond` is false.
///   * `assertMsg(cond, msg)`    — log `msg` then abort.
/// These take the place of the macro for ilang code that wants the
/// usual "give up if an invariant is violated" behaviour.
///
/// The underlying CRT symbols (`__assert_fail`, `_assert`, …) are
/// still bound below as `@optional` for callers that want to invoke
/// the platform handler directly (e.g. to share the same crash
/// dialog / abort path as C-compiled libraries).
///
/// Per-platform symbol names:
///   * glibc:        `__assert_fail(expr, file, line, func)`
///   * musl:         `__assert_fail(expr, file, line, func)`
///   * macOS:        `__assert_rtn(func, file, line, expr)`
///   * msvcrt/ucrt:  `_assert(expr, file, line)`        (CRT)
///                   `_wassert(expr, file, line)`        (wide form)

use stdlib { abort }

/// Abort the process when `cond` is false; no-op otherwise.
/// The C macro variant that prints the failing expression + file +
/// line isn't reproducible at runtime — use `assertMsg` when you
/// want any context in the crash report.
pub fn assert(cond: bool) {
    if !cond { abort() }
}

/// Log `msg` to stdout, then abort. Use when an assertion failure
/// needs to leave a breadcrumb behind.
pub fn assertMsg(cond: bool, msg: string) {
    if !cond {
        console.log(msg)
        abort()
    }
}

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
