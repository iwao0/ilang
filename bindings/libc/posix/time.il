/// POSIX `<time.h>` / `<sys/time.h>` — wall + monotonic clocks.
///
/// Augments libc's ISO `<time.h>` with the POSIX entries (the
/// `clock_*` family, `nanosleep`, `gettimeofday`) that aren't
/// reliably present on the Windows CRT.

@extern(C, "c") {
    /// `struct timespec` — seconds + nanoseconds. Layout is the
    /// POSIX `time_t` + `long` pair; on Windows the matching
    /// type lives in the ucrt headers but isn't exposed by
    /// `clock_gettime` (the symbol itself isn't there).
    pub struct timespec {
        tv_sec:  i64
        tv_nsec: i64
    }

    /// `struct timeval` — seconds + microseconds. Used by
    /// `gettimeofday`, `select`, and a few sysctl APIs.
    pub struct timeval {
        tv_sec:  i64
        tv_usec: i64
    }

    /// POSIX `clock_gettime`. Common `clk_id` values:
    ///   0  CLOCK_REALTIME
    ///   1  CLOCK_MONOTONIC
    ///   2  CLOCK_PROCESS_CPUTIME_ID
    ///   3  CLOCK_THREAD_CPUTIME_ID
    /// Returns 0 on success.
    @lib @optional pub fn clock_gettime(clk_id: i32, tp: *timespec): i32

    /// Set the system time (requires privilege).
    @lib @optional pub fn clock_settime(clk_id: i32, tp: *const timespec): i32

    /// Resolution of `clk_id` written to `*res`.
    @lib @optional pub fn clock_getres(clk_id: i32, res: *timespec): i32

    /// Sleep `clk_id`-relative or absolute (when `flags == 1` /
    /// TIMER_ABSTIME). `remaining` (may be NULL) receives the
    /// unslept remainder if interrupted.
    @lib @optional pub fn clock_nanosleep(
        clk_id:    i32,
        flags:     i32,
        request:   *const timespec,
        remaining: *timespec
    ): i32

    /// POSIX `nanosleep(req, rem)` — sleep with nanosecond
    /// resolution. If interrupted, the remaining time is written
    /// to `*rem`.
    @lib @optional pub fn nanosleep(req: *const timespec, rem: *timespec): i32

    /// `gettimeofday(tv, tz)` — seconds + microseconds since the
    /// Unix epoch. POSIX-deprecated in favour of `clock_gettime`,
    /// but still widely used and present on every Unix host.
    /// `tz` is obsolete; pass `0 as *void`.
    @lib @optional pub fn gettimeofday(tv: *timeval, tz: *void): i32
    @lib @optional pub fn settimeofday(tv: *const timeval, tz: *const void): i32
}
