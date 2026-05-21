/// POSIX `<sys/select.h>` + `<poll.h>` — fd-set I/O multiplexing.
///
/// `fd_set` is a bitset over file descriptors. Layout is
/// implementation-defined; FD_SETSIZE is typically 1024 (glibc /
/// musl / macOS), so a 128-byte bitmap covers every fd in range.
/// Manipulate it only through the FD_* helpers when one is exposed
/// — we don't bind those macros here because they're preprocessor-
/// only; use raw bit ops on the underlying `u64[16]` instead.

use time { timespec, timeval }

@extern(C, "c") {
    pub struct fd_set {
        _bits: u64[16]
    }

    /// `select(nfds, readfds, writefds, exceptfds, timeout)`.
    /// `nfds` must be one more than the highest-numbered fd in
    /// any of the sets. `timeout == NULL` blocks indefinitely;
    /// `tv_sec == 0 && tv_usec == 0` polls.
    @lib @optional pub fn select(
        nfds:      i32,
        readfds:   *fd_set,
        writefds:  *fd_set,
        exceptfds: *fd_set,
        timeout:   *timeval
    ): i32

    /// `pselect` — like `select` but uses `timespec` precision and
    /// atomically swaps the signal mask for the duration of the
    /// call.
    @lib @optional pub fn pselect(
        nfds:      i32,
        readfds:   *fd_set,
        writefds:  *fd_set,
        exceptfds: *fd_set,
        timeout:   *const timespec,
        sigmask:   *const void
    ): i32

    /// `poll`'s per-fd record. `events` is OR-able:
    ///   POLLIN  0x001  POLLPRI  0x002  POLLOUT 0x004
    ///   POLLERR 0x008  POLLHUP  0x010  POLLNVAL 0x020
    pub struct pollfd {
        fd:      i32
        events:  i16
        revents: i16
    }

    /// `poll(fds, nfds, timeout_ms)`. `timeout_ms < 0` blocks
    /// forever; `== 0` polls.
    @lib @optional pub fn poll(fds: *pollfd, nfds: u64, timeout: i32): i32

    /// `ppoll` — POSIX-2008 variant with `timespec` precision +
    /// signal mask.
    @lib @optional pub fn ppoll(
        fds:     *pollfd,
        nfds:    u64,
        timeout: *const timespec,
        sigmask: *const void
    ): i32
}
