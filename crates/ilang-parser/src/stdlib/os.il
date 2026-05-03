// Built-in `os` module — thin host-provided helpers for OS-level
// state that escapes the regular `@extern("lib")` ABI: errno
// inspection / mutation and named POSIX error code constants.

// ─── errno read / write ─────────────────────────────────────────

// Returns the current value of the C `errno` variable (Windows:
// `GetLastError()`). Read after a failing libc call (e.g. `fopen`
// returned NULL) to see *why* it failed.
@extern fn errno(): i32

// Overwrite the current thread's errno. The classic clear-then-call
// pattern: `os.setErrno(0); some_libc_call(); if os.errno() != 0
// { ... }` — useful when the libc fn itself doesn't have a clear
// failure indicator (e.g. some functions return 0 for both "ok"
// and "no entries left").
@extern fn setErrno(code: i32)

// Returns true if `name` was successfully dlopened by any
// `@extern("name") ...` declaration in this program. Used to gate
// calls into libraries declared with the `optional` flag — calling
// a fn whose library failed to load aborts the program, so this
// check runs first:
//   if os.libLoaded("libssl.so.3") {
//       SSL_write(...)
//   }
@extern fn libLoaded(name: string): bool

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
