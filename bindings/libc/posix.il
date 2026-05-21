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

    /// POSIX `nanosleep(req, rem)` — sleep with nanosecond resolution.
    /// If interrupted, the remaining time is written to `*rem`.
    @lib @optional pub fn nanosleep(req: *const timespec, rem: *timespec): i32

    // ── File descriptors (low-level POSIX I/O) ────────────────

    /// `open(path, flags, mode)`. Flags include O_RDONLY (0),
    /// O_WRONLY (1), O_RDWR (2), bit-ored with O_CREAT etc.
    @lib @optional pub fn open(path: *const char, flags: i32, mode: u32): i32
    @lib @optional pub fn close(fd: i32): i32
    @lib @optional pub fn read(fd: i32, buf: *void, count: size_t): i64
    @lib @optional pub fn write(fd: i32, buf: *const void, count: size_t): i64
    @lib @optional pub fn lseek(fd: i32, offset: i64, whence: i32): i64
    @lib @optional pub fn dup(oldfd: i32): i32
    @lib @optional pub fn dup2(oldfd: i32, newfd: i32): i32
    @lib @optional pub fn pipe(fds: *i32): i32
    @lib @optional pub fn isatty(fd: i32): i32

    // ── Filesystem ────────────────────────────────────────────

    /// Remove a regular file. Same as `stdio.remove` for files.
    @lib @optional pub fn unlink(path: *const char): i32

    /// `access(path, mode)`. Modes: F_OK 0, X_OK 1, W_OK 2, R_OK 4.
    @lib @optional pub fn access(path: *const char, mode: i32): i32

    /// Current working directory. Returns `buf` on success, NULL on
    /// failure (ERANGE if `size` is too small).
    @lib @optional pub fn getcwd(buf: *char, size: size_t): *char

    @lib @optional pub fn chdir(path: *const char): i32
    @lib @optional pub fn mkdir(path: *const char, mode: u32): i32
    @lib @optional pub fn rmdir(path: *const char): i32
    @lib @optional pub fn symlink(target: *const char, linkpath: *const char): i32
    @lib @optional pub fn readlink(path: *const char, buf: *char, bufsize: size_t): i64
    @lib @optional pub fn chmod(path: *const char, mode: u32): i32

    // ── Process control ──────────────────────────────────────

    /// Process IDs. POSIX `pid_t` is typically `int`.
    @lib @optional pub fn getpid(): i32
    @lib @optional pub fn getppid(): i32
    @lib @optional pub fn getuid(): u32
    @lib @optional pub fn geteuid(): u32
    @lib @optional pub fn getgid(): u32
    @lib @optional pub fn getegid(): u32

    /// Fork the current process. Returns 0 in the child, child PID
    /// in the parent, -1 on error.
    @lib @optional pub fn fork(): i32

    /// `execv(path, argv)` — replace the current process image.
    @lib @optional pub fn execv(path: *const char, argv: **const char): i32
    @lib @optional pub fn execvp(file: *const char, argv: **const char): i32

    /// Wait for `pid` to change state. `pid == -1` means any child.
    /// Returns the PID of the reaped child or -1 on error.
    @lib @optional pub fn waitpid(pid: i32, wstatus: *i32, options: i32): i32

    /// Send `sig` to `pid`. `pid == 0` ≡ same process group;
    /// `pid == -1` ≡ all permitted processes.
    @lib @optional pub fn kill(pid: i32, sig: i32): i32

    // ── Terminal / TTY ────────────────────────────────────────

    /// Returns the basename of the controlling terminal as a pointer
    /// into a static buffer, or NULL.
    @lib @optional pub fn ttyname(fd: i32): *const char
}
