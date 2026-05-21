/// POSIX-only entry points that ship as part of libc on Linux /
/// macOS but are *missing* (or under different names) on
/// Windows's CRT.
///
/// Every fn here is annotated `@optional`: on hosts where the
/// symbol is absent, the AOT linker substitutes an abort-stub and
/// the JIT raises an "unresolved symbol" panic only when the fn is
/// actually called. Reach for the kernel32 / msvcrt-specific
/// alternative on Windows when portability matters
/// (e.g. `_aligned_malloc` vs `posix_memalign`,
/// `QueryPerformanceCounter` vs `clock_gettime`).

@extern(C, "c") {
    /// `struct timespec` — seconds + nanoseconds. Layout is the
    /// POSIX `time_t` + `long` pair; on Windows the matching
    /// type lives in the ucrt headers but isn't exposed by
    /// `clock_gettime` (the symbol itself isn't there).
    pub struct timespec {
        tv_sec:  i64
        tv_nsec: i64
    }

    /// POSIX `clock_gettime`. Common `clk_id` values:
    ///   0  CLOCK_REALTIME
    ///   1  CLOCK_MONOTONIC
    ///   2  CLOCK_PROCESS_CPUTIME_ID
    ///   3  CLOCK_THREAD_CPUTIME_ID
    /// Returns 0 on success.
    @lib @optional pub fn clock_gettime(clk_id: i32, tp: *timespec): i32

    /// Aligned allocation. `alignment` must be a power of two
    /// and a multiple of `sizeof(void*)`. Free via `free`.
    /// Returns 0 on success and writes the new block to `*memptr`.
    @lib @optional pub fn posix_memalign(
        memptr:    *i64,
        alignment: size_t,
        size:      size_t
    ): i32

    /// Duplicate a NUL-terminated string into a fresh `malloc`'d
    /// buffer. Free with `free`. POSIX since 2008; ucrt exposes it
    /// as `_strdup` (without the leading underscore the symbol
    /// isn't present on most Windows builds).
    @lib @optional pub fn strdup(s: *const char): *char

    /// `gettimeofday(tv, tz)` — seconds + microseconds since the
    /// Unix epoch. POSIX-deprecated in favour of `clock_gettime`,
    /// but still widely used and present on every Unix host.
    @lib @optional pub fn gettimeofday(tv: *void, tz: *void): i32

    /// POSIX `fileno(stream)` → underlying OS file descriptor.
    /// Windows exposes the same symbol as `_fileno`.
    @lib @optional pub fn fileno(stream: *void): i32

    /// Sleep the current thread for `seconds`. Returns 0 on full
    /// completion, the number of seconds left if interrupted.
    @lib @optional pub fn sleep(seconds: u32): u32

    /// Microsecond-precision sleep. POSIX-only.
    @lib @optional pub fn usleep(usec: u32): i32
}
