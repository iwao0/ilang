/// C standard `<time.h>` — wall + process clocks, calendar utils.

@extern(C, "c") {
    /// Broken-down time, as returned by `gmtime` / `localtime`.
    /// Fields are inclusive of negative values where the C SDK
    /// permits them (`tm_year` is years since 1900).
    pub struct tm {
        tm_sec:   i32
        tm_min:   i32
        tm_hour:  i32
        tm_mday:  i32
        tm_mon:   i32
        tm_year:  i32
        tm_wday:  i32
        tm_yday:  i32
        tm_isdst: i32
    }

    // ── Wall clock (seconds since the Unix epoch on POSIX, since
    //    Jan 1 1970 UTC on Windows ucrt as well) ─────────────────

    /// Seconds since the epoch. Pass NULL for `tloc` to discard
    /// the secondary out-parameter copy.
    @lib pub fn time(tloc: *i64): i64

    /// Difference in seconds (`end - begin`). Resilient to wrap
    /// in places where `time_t` is unsigned.
    @lib pub fn difftime(end: i64, begin: i64): f64

    // ── Calendar conversions ───────────────────────────────────

    /// `time_t` → broken-down UTC time. Returns NULL on overflow.
    @lib pub fn gmtime(timep: *const i64): *tm

    /// `time_t` → broken-down local time (honours TZ env / system
    /// locale). Returns NULL on overflow.
    @lib pub fn localtime(timep: *const i64): *tm

    /// Reverse of `localtime` — broken-down → `time_t`. Normalises
    /// out-of-range fields (`mktime(tm{ .tm_mday = 32 })` ≈ next month).
    @lib pub fn mktime(tm: *tm): i64

    /// Format a `tm` per `fmt` (`%Y-%m-%d` etc.). Returns the
    /// number of bytes written (excluding NUL), 0 if the buffer
    /// was too small.
    @lib pub fn strftime(
        buf:    *char,
        maxsize: size_t,
        fmt:    *const char,
        tm:     *const tm
    ): size_t

    /// Default text form ("Wed Jun 30 21:49:08 1993\n"). 26-byte
    /// buffer required by spec; returns a pointer into a static
    /// buffer. Not thread-safe.
    @lib pub fn ctime(timep: *const i64): *const char

    /// Same as `ctime` but takes a `tm` directly.
    @lib pub fn asctime(tm: *const tm): *const char

    // ── Process clock ─────────────────────────────────────────

    /// Approximate CPU time the process has consumed, in
    /// `CLOCKS_PER_SEC` ticks. Returns -1 if unavailable.
    @lib pub fn clock(): i64

    // ── C11 `timespec_get` ────────────────────────────────────

    /// C11 wall-clock read. `base` is typically `1` (TIME_UTC).
    /// Returns `base` on success, 0 on failure.
    @lib @optional pub fn timespec_get(ts: *void, base: i32): i32
}
