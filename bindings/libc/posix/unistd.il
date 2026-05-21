/// POSIX `<unistd.h>` — file descriptors, process IDs, fork / exec.
///
/// Every fn here is annotated `@optional`: on hosts where the
/// symbol is absent, the AOT linker substitutes an abort-stub and
/// the JIT raises an "unresolved symbol" panic only when the fn is
/// actually called. Reach for the kernel32 / msvcrt-specific
/// alternative on Windows when portability matters.

@extern(C, "c") {
    // ── File descriptors (low-level POSIX I/O) ────────────────

    /// `open(path, flags, mode)`. Flags include O_RDONLY (0),
    /// O_WRONLY (1), O_RDWR (2), bit-ored with O_CREAT etc.
    @lib @optional pub fn open(path: *const char, flags: i32, mode: u32): i32
    @lib @optional pub fn close(fd: i32): i32
    @lib @optional pub fn read(fd: i32, buf: *void, count: size_t): i64
    @lib @optional pub fn write(fd: i32, buf: *const void, count: size_t): i64

    /// Same as `read`/`write` but at an explicit offset (no seek).
    @lib @optional pub fn pread(fd: i32, buf: *void, count: size_t, offset: i64): i64
    @lib @optional pub fn pwrite(fd: i32, buf: *const void, count: size_t, offset: i64): i64

    /// `lseek(fd, offset, whence)`. `whence`: SEEK_SET 0, SEEK_CUR 1,
    /// SEEK_END 2.
    @lib @optional pub fn lseek(fd: i32, offset: i64, whence: i32): i64

    @lib @optional pub fn dup(oldfd: i32): i32
    @lib @optional pub fn dup2(oldfd: i32, newfd: i32): i32
    @lib @optional pub fn pipe(fds: *i32): i32
    @lib @optional pub fn isatty(fd: i32): i32
    @lib @optional pub fn ftruncate(fd: i32, length: i64): i32
    @lib @optional pub fn fsync(fd: i32): i32
    @lib @optional pub fn fdatasync(fd: i32): i32

    // ── Filesystem ────────────────────────────────────────────

    /// Remove a regular file. Same as `stdio.remove` for files.
    @lib @optional pub fn unlink(path: *const char): i32

    /// `access(path, mode)`. Modes: F_OK 0, X_OK 1, W_OK 2, R_OK 4.
    @lib @optional pub fn access(path: *const char, mode: i32): i32

    /// Current working directory. Returns `buf` on success, NULL on
    /// failure (ERANGE if `size` is too small).
    @lib @optional pub fn getcwd(buf: *char, size: size_t): *char

    @lib @optional pub fn chdir(path: *const char): i32
    @lib @optional pub fn fchdir(fd: i32): i32
    @lib @optional pub fn rmdir(path: *const char): i32
    @lib @optional pub fn symlink(target: *const char, linkpath: *const char): i32
    @lib @optional pub fn readlink(path: *const char, buf: *char, bufsize: size_t): i64
    @lib @optional pub fn link(oldpath: *const char, newpath: *const char): i32
    @lib @optional pub fn chown(path: *const char, owner: u32, group: u32): i32
    @lib @optional pub fn fchown(fd: i32, owner: u32, group: u32): i32
    @lib @optional pub fn lchown(path: *const char, owner: u32, group: u32): i32
    @lib @optional pub fn truncate(path: *const char, length: i64): i32

    // ── Process control ──────────────────────────────────────

    /// Process IDs. POSIX `pid_t` is typically `int`.
    @lib @optional pub fn getpid(): i32
    @lib @optional pub fn getppid(): i32
    @lib @optional pub fn getuid(): u32
    @lib @optional pub fn geteuid(): u32
    @lib @optional pub fn getgid(): u32
    @lib @optional pub fn getegid(): u32
    @lib @optional pub fn setuid(uid: u32): i32
    @lib @optional pub fn setgid(gid: u32): i32
    @lib @optional pub fn seteuid(uid: u32): i32
    @lib @optional pub fn setegid(gid: u32): i32

    /// Get / set the process group ID and session ID.
    @lib @optional pub fn getpgid(pid: i32): i32
    @lib @optional pub fn setpgid(pid: i32, pgid: i32): i32
    @lib @optional pub fn getsid(pid: i32): i32
    @lib @optional pub fn setsid(): i32

    /// Fork the current process. Returns 0 in the child, child PID
    /// in the parent, -1 on error.
    @lib @optional pub fn fork(): i32

    /// `execv(path, argv)` — replace the current process image.
    @lib @optional pub fn execv(path: *const char, argv: **const char): i32
    @lib @optional pub fn execvp(file: *const char, argv: **const char): i32
    @lib @optional pub fn execve(
        path: *const char,
        argv: **const char,
        envp: **const char
    ): i32

    // ── Terminal / TTY ────────────────────────────────────────

    /// Returns the basename of the controlling terminal as a pointer
    /// into a static buffer, or NULL.
    @lib @optional pub fn ttyname(fd: i32): *const char

    /// `getlogin()` — user name attached to the controlling terminal.
    /// Pointer into a static buffer; NULL on failure.
    @lib @optional pub fn getlogin(): *const char

    // ── Sleep ─────────────────────────────────────────────────

    /// Sleep the current thread for `seconds`. Returns 0 on full
    /// completion, the number of seconds left if interrupted.
    @lib @optional pub fn sleep(seconds: u32): u32

    /// Microsecond-precision sleep. POSIX-only.
    @lib @optional pub fn usleep(usec: u32): i32

    // ── Misc ──────────────────────────────────────────────────

    /// Returns the host name. Returns 0 on success.
    @lib @optional pub fn gethostname(name: *char, len: size_t): i32

    /// Query system configuration. `name`: _SC_PAGESIZE (29 on
    /// glibc), _SC_NPROCESSORS_ONLN (84) etc. Returns -1 if `name`
    /// is invalid; otherwise the configured value.
    @lib @optional pub fn sysconf(name: i32): i64

    /// Page size in bytes. Equivalent to `sysconf(_SC_PAGESIZE)`.
    @lib @optional pub fn getpagesize(): i32
}
