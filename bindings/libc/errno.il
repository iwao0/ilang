/// C standard `<errno.h>` — last-error retrieval.
///
/// `errno` itself is a thread-local lvalue in C, exposed through
/// different accessor functions on each platform:
///
///   * glibc / musl:  `__errno_location() -> int*`
///   * macOS:         `__error()           -> int*`
///   * msvcrt / ucrt: `_errno()            -> int*`
///
/// All three are bound here as `@optional`; whichever symbol is
/// present at link time wins. Read / write the value through the
/// returned pointer:
///
///   let p = _errno()
///   if p != 0 as *i32 { let err = *p }
///
/// Common POSIX values (decimal):
///   EPERM     1   EBADF    9   EINVAL  22
///   ENOENT    2   EAGAIN  11   EMFILE  24
///   ESRCH     3   ENOMEM  12   ENOSPC  28
///   EINTR     4   EACCES  13   EPIPE   32
///   EIO       5   EEXIST  17   ERANGE  34
///   ENXIO     6   ENOTDIR 20
///   EBUSY    16   EISDIR  21
///
/// Values differ between Linux, macOS, and Windows — when you need
/// a specific code on a specific OS, look it up in the headers.

@extern(C, "c") {
    @lib @optional pub fn __errno_location(): *i32
    @lib @optional pub fn __error(): *i32
    @lib @optional pub fn _errno(): *i32
}
