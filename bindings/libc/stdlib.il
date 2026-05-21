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
}
