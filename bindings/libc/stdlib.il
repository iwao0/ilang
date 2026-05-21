/// C standard `<stdlib.h>` — memory + integer / process / env helpers.

@extern(C, "c") {
    // ── Memory ──────────────────────────────────────────────────

    /// Allocate `size` bytes. Returns NULL on failure. Free with `free`.
    @lib pub fn malloc(size: size_t): *void

    /// Allocate `nmemb * size` bytes initialised to zero.
    @lib pub fn calloc(nmemb: size_t, size: size_t): *void

    /// Resize the block at `ptr` to `size` bytes. May return a
    /// different pointer; the caller must replace the original.
    @lib pub fn realloc(ptr: *void, size: size_t): *void

    /// Release a block previously returned by malloc / calloc /
    /// realloc / strdup. Passing NULL is defined and a no-op.
    @lib pub fn free(ptr: *void)

    // ── Integer arithmetic ──────────────────────────────────────

    /// Absolute value of an `int`. Defined for non-INT_MIN inputs;
    /// `abs(INT_MIN)` is UB by spec.
    @lib pub fn abs(x: i32): i32

    /// `long` absolute value.
    @lib pub fn labs(x: i64): i64

    // ── String → number ─────────────────────────────────────────

    /// Parse decimal `int` from a NUL-terminated string. Returns 0
    /// on parse failure. Use `strtol` if error detection is needed.
    @lib pub fn atoi(s: *const char): i32

    /// `long` parse — same caveats as `atoi`.
    @lib pub fn atol(s: *const char): i64

    /// `double` parse.
    @lib pub fn atof(s: *const char): f64

    /// `strtol(s, &endp, base)` — parse `long` with arbitrary base
    /// (0 = autodetect). `endp` (out) points at the first
    /// unparsed character.
    @lib pub fn strtol(s: *const char, endptr: **char, base: i32): i64

    @lib pub fn strtoul(s: *const char, endptr: **char, base: i32): u64
    @lib pub fn strtod(s: *const char, endptr: **char): f64

    // ── Process control ─────────────────────────────────────────

    /// Terminate the process with `status` as the exit code.
    /// Runs `atexit` handlers, flushes / closes stdio streams.
    @lib pub fn exit(status: i32)

    /// Abrupt termination — no atexit / no stdio flush. Raises
    /// SIGABRT on POSIX; on Windows it pops the crash dialog.
    @lib pub fn abort()

    /// Read an environment variable. Returns NULL when unset.
    /// On Windows this is the ANSI view; for the wide view use
    /// `kernel32.GetEnvironmentVariableW`.
    @lib pub fn getenv(name: *const char): *const char

    /// Run `cmd` through the system command interpreter
    /// (`/bin/sh -c cmd` on POSIX, `cmd.exe /c cmd` on Windows).
    /// Returns the child's exit status, or -1 on failure.
    @lib pub fn system(cmd: *const char): i32

    // ── Sorting / search ─────────────────────────────────────────

    /// In-place sort of `nmemb` elements of `size` bytes each.
    /// `cmp` returns negative / 0 / positive like `strcmp`.
    @lib pub fn qsort(
        base:  *void,
        nmemb: size_t,
        size:  size_t,
        cmp:   fn(*const void, *const void): i32
    )

    /// Binary search over a sorted run of `nmemb` × `size` bytes.
    /// Returns NULL when no element matches.
    @lib pub fn bsearch(
        key:   *const void,
        base:  *const void,
        nmemb: size_t,
        size:  size_t,
        cmp:   fn(*const void, *const void): i32
    ): *void

    // ── Random numbers (low-quality / cross-platform default) ────

    /// Seed the PRNG. POSIX permits 32-bit; Windows allows any.
    @lib pub fn srand(seed: u32)

    /// Pull a non-negative pseudo-random `int` ≤ `RAND_MAX`
    /// (Windows: 0x7FFF; glibc: 0x7FFFFFFF — see `RAND_MAX`).
    @lib pub fn rand(): i32

    // ── div / quotient + remainder pairs ────────────────────────

    /// Quotient + remainder pair for `int` division.
    pub struct div_t {
        quot: i32
        rem:  i32
    }

    /// Quotient + remainder pair for `long` division.
    pub struct ldiv_t {
        quot: i64
        rem:  i64
    }

    /// Quotient + remainder pair for `long long` division.
    pub struct lldiv_t {
        quot: i64
        rem:  i64
    }

    @lib pub fn div(numer: i32, denom: i32): div_t
    @lib pub fn ldiv(numer: i64, denom: i64): ldiv_t
    @lib pub fn lldiv(numer: i64, denom: i64): lldiv_t

    /// `long long` absolute value (C99).
    @lib pub fn llabs(x: i64): i64

    // ── Additional string → number ──────────────────────────────

    /// C99 `long long` parse (decimal only).
    @lib pub fn atoll(s: *const char): i64

    /// `strtoll(s, &endp, base)` — C99.
    @lib pub fn strtoll(s: *const char, endptr: **char, base: i32): i64
    @lib pub fn strtoull(s: *const char, endptr: **char, base: i32): u64

    /// `float` parse (C99).
    @lib pub fn strtof(s: *const char, endptr: **char): f32

    // ── Process control (additions) ─────────────────────────────

    /// Register a function to run at normal program termination.
    /// Returns 0 on success.
    @lib pub fn atexit(func: fn()): i32

    /// C99 quick-exit — skips atexit handlers, still flushes stdio
    /// on most implementations.
    @lib @optional pub fn _Exit(status: i32)

    /// Set / clear an environment variable. POSIX, but ucrt exposes
    /// the same name. Returns 0 on success.
    @lib @optional pub fn setenv(name: *const char, value: *const char, overwrite: i32): i32
    @lib @optional pub fn unsetenv(name: *const char): i32

    // ── Multi-byte ↔ wide conversion ────────────────────────────

    /// Length in bytes of the next multi-byte character starting at
    /// `s`. Returns -1 on encoding error, 0 for the empty string.
    @lib pub fn mblen(s: *const char, n: size_t): i32

    /// Decode one multi-byte character.
    @lib pub fn mbtowc(pwc: *u32, s: *const char, n: size_t): i32

    /// Encode one wide character as multi-byte bytes.
    @lib pub fn wctomb(s: *char, wc: u32): i32

    /// Convert a multi-byte string to wide. Returns the number of
    /// wide chars written (excluding the NUL) or (size_t)-1.
    @lib pub fn mbstowcs(pwcs: *u32, s: *const char, n: size_t): size_t
    @lib pub fn wcstombs(s: *char, pwcs: *const u32, n: size_t): size_t
}
