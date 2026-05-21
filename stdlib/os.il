/// Built-in `os` module — thin host-provided helpers for OS-level
/// state that escapes the regular `@extern("lib")` ABI: errno
/// inspection / mutation and named POSIX error code constants.

// ─── errno read / write ─────────────────────────────────────────

@extern(C) {
    /// Returns the current value of the C `errno` variable
    /// (Windows: `GetLastError()`). Read after a failing libc call
    /// (e.g. `fopen` returned NULL) to see *why* it failed.
    pub fn errno(): i32

    /// Overwrite the current thread's errno. The classic
    /// clear-then-call pattern:
    ///   os.setErrno(0); some_libc_call(); if os.errno() != 0 { ... }
    /// — useful when the libc fn itself doesn't have a clear failure
    /// indicator (e.g. some functions return 0 for both "ok" and
    /// "no entries left").
    pub fn setErrno(code: i32)

    /// Returns true if `name` was successfully dlopened by any
    /// `@lib("name") ...` declaration in this program. Used to gate
    /// calls into libraries declared `@optional` — calling a fn
    /// whose library failed to load aborts the program, so this
    /// check runs first:
    ///   if os.libLoaded("libssl.so.3") { SSL_write(...) }
    pub fn libLoaded(name: string): bool

    /// Returns the dlopen error message captured for a missing
    /// `@lib(...) @optional` library, or "" if the library loaded
    /// successfully (or no `@lib` ever attempted to load it). Only
    /// useful for diagnostic logging; gating logic should use
    /// `os.libLoaded(...)`.
    pub fn libLoadError(name: string): string

    /// Host OS name probe — called exactly once during module
    /// init to populate the `os.platform` binding below. Don't
    /// call this directly; reach for `os.platform` instead.
    pub fn __platform(): string
}

/// Host OS name. One of `"macos"` / `"linux"` / `"windows"`,
/// or `"other"` for any other Rust-known target. Resolved at
/// compile time from `cfg(target_os)`; the helper runs once
/// at module init and the value is cached here for the rest of
/// the program. Use it to branch on platform without writing a
/// conditional-compilation attribute:
///
///     if os.platform == "windows" { ... }
pub let platform: string = __platform()

// ─── POSIX error code constants ─────────────────────────────────
// Values chosen to match macOS / Linux glibc — these 30-something
// codes have identical numeric values across both platforms. The
// platform-divergent codes (EAGAIN, EWOULDBLOCK, ENOTSUP, …) are
// intentionally omitted; consult the platform header if you need
// one of those.
//
// Cross-checked against `errno.h` on macOS 14 (Darwin) and Linux
// 6.x (glibc).

pub const EPERM: i32 = 1     // Operation not permitted
pub const ENOENT: i32 = 2    // No such file or directory
pub const ESRCH: i32 = 3     // No such process
pub const EINTR: i32 = 4     // Interrupted system call
pub const EIO: i32 = 5       // I/O error
pub const ENXIO: i32 = 6     // No such device or address
pub const E2BIG: i32 = 7     // Argument list too long
pub const ENOEXEC: i32 = 8   // Exec format error
pub const EBADF: i32 = 9     // Bad file descriptor
pub const ECHILD: i32 = 10   // No child processes
pub const ENOMEM: i32 = 12   // Cannot allocate memory
pub const EACCES: i32 = 13   // Permission denied
pub const EFAULT: i32 = 14   // Bad address
pub const EBUSY: i32 = 16    // Device / resource busy
pub const EEXIST: i32 = 17   // File exists
pub const EXDEV: i32 = 18    // Cross-device link
pub const ENODEV: i32 = 19   // No such device
pub const ENOTDIR: i32 = 20  // Not a directory
pub const EISDIR: i32 = 21   // Is a directory
pub const EINVAL: i32 = 22   // Invalid argument
pub const ENFILE: i32 = 23   // Too many open files in system
pub const EMFILE: i32 = 24   // Too many open files (per process)
pub const ENOTTY: i32 = 25   // Inappropriate ioctl for device
pub const ETXTBSY: i32 = 26  // Text file busy
pub const EFBIG: i32 = 27    // File too large
pub const ENOSPC: i32 = 28   // No space left on device
pub const ESPIPE: i32 = 29   // Illegal seek
pub const EROFS: i32 = 30    // Read-only file system
pub const EMLINK: i32 = 31   // Too many links
pub const EPIPE: i32 = 32    // Broken pipe
pub const EDOM: i32 = 33     // Numerical argument out of domain
pub const ERANGE: i32 = 34   // Numerical result out of range

// ─── Standard file descriptors ──────────────────────────────────
pub const STDIN_FILENO: i32 = 0
pub const STDOUT_FILENO: i32 = 1
pub const STDERR_FILENO: i32 = 2

// ─── Process exit codes ─────────────────────────────────────────
pub const EXIT_SUCCESS: i32 = 0
pub const EXIT_FAILURE: i32 = 1

// ─── lseek whence ───────────────────────────────────────────────
pub const SEEK_SET: i32 = 0   // from start
pub const SEEK_CUR: i32 = 1   // from current position
pub const SEEK_END: i32 = 2   // from end

// ─── open() flags ───────────────────────────────────────────────
// Access mode (low 2 bits, mutually exclusive).
pub const O_RDONLY: i32 = 0
pub const O_WRONLY: i32 = 1
pub const O_RDWR: i32 = 2
// Modifier flags (OR with access mode). NOTE: O_CREAT / O_TRUNC /
// O_APPEND have **different numeric values on Linux vs macOS**.
// The constants below are macOS values — programs needing Linux
// portability should use libc names directly via @extern. Most
// portable values:
pub const O_NONBLOCK: i32 = 4    // both Linux and macOS
pub const O_APPEND: i32 = 8      // both Linux and macOS

// ─── File mode bits (POSIX, identical on Linux/macOS) ───────────
pub const S_IRUSR: i32 = 256    // 0o400 — owner read
pub const S_IWUSR: i32 = 128    // 0o200 — owner write
pub const S_IXUSR: i32 = 64     // 0o100 — owner execute
pub const S_IRGRP: i32 = 32     // 0o040 — group read
pub const S_IWGRP: i32 = 16     // 0o020 — group write
pub const S_IXGRP: i32 = 8      // 0o010 — group execute
pub const S_IROTH: i32 = 4      // 0o004 — other read
pub const S_IWOTH: i32 = 2      // 0o002 — other write
pub const S_IXOTH: i32 = 1      // 0o001 — other execute

// ─── Socket address families (AF_*) ─────────────────────────────
// These differ across BSD/Linux for some, but the common ones agree:
pub const AF_UNIX: i32 = 1
pub const AF_INET: i32 = 2
pub const AF_INET6: i32 = 30   // macOS = 30, Linux = 10 — TODO: gate per OS

// ─── Socket types (SOCK_*) ──────────────────────────────────────
pub const SOCK_STREAM: i32 = 1
pub const SOCK_DGRAM: i32 = 2
pub const SOCK_RAW: i32 = 3

// ─── Common signals (POSIX, identical on Linux/macOS) ───────────
pub const SIGINT: i32 = 2     // Ctrl-C
pub const SIGQUIT: i32 = 3
pub const SIGILL: i32 = 4
pub const SIGABRT: i32 = 6
pub const SIGFPE: i32 = 8
pub const SIGKILL: i32 = 9
pub const SIGSEGV: i32 = 11
pub const SIGPIPE: i32 = 13
pub const SIGALRM: i32 = 14
pub const SIGTERM: i32 = 15
