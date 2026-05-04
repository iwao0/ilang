// Built-in `os` module — thin host-provided helpers for OS-level
// state that escapes the regular `@extern("lib")` ABI: errno
// inspection / mutation and named POSIX error code constants.

// ─── errno read / write ─────────────────────────────────────────

@extern(C) {
    // Returns the current value of the C `errno` variable
    // (Windows: `GetLastError()`). Read after a failing libc call
    // (e.g. `fopen` returned NULL) to see *why* it failed.
    fn errno(): i32

    // Overwrite the current thread's errno. The classic
    // clear-then-call pattern:
    //   os.setErrno(0); some_libc_call(); if os.errno() != 0 { ... }
    // — useful when the libc fn itself doesn't have a clear failure
    // indicator (e.g. some functions return 0 for both "ok" and
    // "no entries left").
    fn setErrno(code: i32)

    // Returns true if `name` was successfully dlopened by any
    // `@lib("name") ...` declaration in this program. Used to gate
    // calls into libraries declared `@optional` — calling a fn
    // whose library failed to load aborts the program, so this
    // check runs first:
    //   if os.libLoaded("libssl.so.3") { SSL_write(...) }
    fn libLoaded(name: string): bool

    // Returns the dlopen error message captured for a missing
    // `@lib(...) @optional` library, or "" if the library loaded
    // successfully (or no `@lib` ever attempted to load it). Only
    // useful for diagnostic logging; gating logic should use
    // `os.libLoaded(...)`.
    fn libLoadError(name: string): string
}

// ─── POSIX error code constants ─────────────────────────────────
// Values chosen to match macOS / Linux glibc — these 30-something
// codes have identical numeric values across both platforms. The
// platform-divergent codes (EAGAIN, EWOULDBLOCK, ENOTSUP, …) are
// intentionally omitted; consult the platform header if you need
// one of those.
//
// Cross-checked against `errno.h` on macOS 14 (Darwin) and Linux
// 6.x (glibc).

const EPERM: i32 = 1     // Operation not permitted
const ENOENT: i32 = 2    // No such file or directory
const ESRCH: i32 = 3     // No such process
const EINTR: i32 = 4     // Interrupted system call
const EIO: i32 = 5       // I/O error
const ENXIO: i32 = 6     // No such device or address
const E2BIG: i32 = 7     // Argument list too long
const ENOEXEC: i32 = 8   // Exec format error
const EBADF: i32 = 9     // Bad file descriptor
const ECHILD: i32 = 10   // No child processes
const ENOMEM: i32 = 12   // Cannot allocate memory
const EACCES: i32 = 13   // Permission denied
const EFAULT: i32 = 14   // Bad address
const EBUSY: i32 = 16    // Device / resource busy
const EEXIST: i32 = 17   // File exists
const EXDEV: i32 = 18    // Cross-device link
const ENODEV: i32 = 19   // No such device
const ENOTDIR: i32 = 20  // Not a directory
const EISDIR: i32 = 21   // Is a directory
const EINVAL: i32 = 22   // Invalid argument
const ENFILE: i32 = 23   // Too many open files in system
const EMFILE: i32 = 24   // Too many open files (per process)
const ENOTTY: i32 = 25   // Inappropriate ioctl for device
const ETXTBSY: i32 = 26  // Text file busy
const EFBIG: i32 = 27    // File too large
const ENOSPC: i32 = 28   // No space left on device
const ESPIPE: i32 = 29   // Illegal seek
const EROFS: i32 = 30    // Read-only file system
const EMLINK: i32 = 31   // Too many links
const EPIPE: i32 = 32    // Broken pipe
const EDOM: i32 = 33     // Numerical argument out of domain
const ERANGE: i32 = 34   // Numerical result out of range

// ─── Standard file descriptors ──────────────────────────────────
const STDIN_FILENO: i32 = 0
const STDOUT_FILENO: i32 = 1
const STDERR_FILENO: i32 = 2

// ─── Process exit codes ─────────────────────────────────────────
const EXIT_SUCCESS: i32 = 0
const EXIT_FAILURE: i32 = 1

// ─── lseek whence ───────────────────────────────────────────────
const SEEK_SET: i32 = 0   // from start
const SEEK_CUR: i32 = 1   // from current position
const SEEK_END: i32 = 2   // from end

// ─── open() flags ───────────────────────────────────────────────
// Access mode (low 2 bits, mutually exclusive).
const O_RDONLY: i32 = 0
const O_WRONLY: i32 = 1
const O_RDWR: i32 = 2
// Modifier flags (OR with access mode). NOTE: O_CREAT / O_TRUNC /
// O_APPEND have **different numeric values on Linux vs macOS**.
// The constants below are macOS values — programs needing Linux
// portability should use libc names directly via @extern. Most
// portable values:
const O_NONBLOCK: i32 = 4    // both Linux and macOS
const O_APPEND: i32 = 8      // both Linux and macOS

// ─── File mode bits (POSIX, identical on Linux/macOS) ───────────
const S_IRUSR: i32 = 256    // 0o400 — owner read
const S_IWUSR: i32 = 128    // 0o200 — owner write
const S_IXUSR: i32 = 64     // 0o100 — owner execute
const S_IRGRP: i32 = 32     // 0o040 — group read
const S_IWGRP: i32 = 16     // 0o020 — group write
const S_IXGRP: i32 = 8      // 0o010 — group execute
const S_IROTH: i32 = 4      // 0o004 — other read
const S_IWOTH: i32 = 2      // 0o002 — other write
const S_IXOTH: i32 = 1      // 0o001 — other execute

// ─── Socket address families (AF_*) ─────────────────────────────
// These differ across BSD/Linux for some, but the common ones agree:
const AF_UNIX: i32 = 1
const AF_INET: i32 = 2
const AF_INET6: i32 = 30   // macOS = 30, Linux = 10 — TODO: gate per OS

// ─── Socket types (SOCK_*) ──────────────────────────────────────
const SOCK_STREAM: i32 = 1
const SOCK_DGRAM: i32 = 2
const SOCK_RAW: i32 = 3

// ─── Common signals (POSIX, identical on Linux/macOS) ───────────
const SIGINT: i32 = 2     // Ctrl-C
const SIGQUIT: i32 = 3
const SIGILL: i32 = 4
const SIGABRT: i32 = 6
const SIGFPE: i32 = 8
const SIGKILL: i32 = 9
const SIGSEGV: i32 = 11
const SIGPIPE: i32 = 13
const SIGALRM: i32 = 14
const SIGTERM: i32 = 15
