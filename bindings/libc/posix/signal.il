/// POSIX `<signal.h>` extensions — sigaction, signal sets, masks.
///
/// Augments the ISO `<signal.h>` (`signal` / `raise`) which lives
/// in `libc.signal`. The POSIX surface adds:
///   * `sigaction` — install handlers with full siginfo + sa_mask
///   * `sigprocmask` — block / unblock signals on the current thread
///   * `sigemptyset` / `sigaddset` / etc. — manipulate `sigset_t`
///
/// `sigset_t` layout: glibc `unsigned long[16]` (128 bytes), musl
/// uses 128 too; macOS uses a single `u32`. We reserve 128 bytes
/// so all platforms fit. Manipulate it only through the provided
/// functions.

@extern(C, "c") {
    /// Opaque signal set.
    pub struct sigset_t {
        _opaque: u8[128]
    }

    /// `struct sigaction`. Layout differs noticeably between glibc
    /// and musl (sa_handler is a union with sa_sigaction in C; we
    /// expose only the simple handler form). Sa_flags = SA_RESTART
    /// (0x10000000) to auto-restart interrupted syscalls.
    pub struct sigaction {
        sa_handler: fn(i32)
        sa_mask:    sigset_t
        sa_flags:   i32
        sa_restorer: *void   // glibc-only, ignored elsewhere
    }

    @lib @optional pub fn sigaction(
        signum: i32,
        act:    *const sigaction,
        oldact: *sigaction
    ): i32

    /// `sigprocmask(how, set, oldset)`. `how`:
    ///   SIG_BLOCK    0
    ///   SIG_UNBLOCK  1
    ///   SIG_SETMASK  2
    @lib @optional pub fn sigprocmask(
        how:    i32,
        set:    *const sigset_t,
        oldset: *sigset_t
    ): i32

    @lib @optional pub fn pthread_sigmask(
        how:    i32,
        set:    *const sigset_t,
        oldset: *sigset_t
    ): i32

    /// Wait synchronously until one of the signals in `set` is
    /// pending; returns the signal number.
    @lib @optional pub fn sigwait(set: *const sigset_t, sig: *i32): i32

    /// Pending signals on the calling thread.
    @lib @optional pub fn sigpending(set: *sigset_t): i32

    /// Suspend until a signal arrives.
    @lib @optional pub fn sigsuspend(mask: *const sigset_t): i32

    // ── sigset_t manipulation ──────────────────────────────

    @lib @optional pub fn sigemptyset(set: *sigset_t): i32
    @lib @optional pub fn sigfillset(set: *sigset_t): i32
    @lib @optional pub fn sigaddset(set: *sigset_t, signum: i32): i32
    @lib @optional pub fn sigdelset(set: *sigset_t, signum: i32): i32
    @lib @optional pub fn sigismember(set: *const sigset_t, signum: i32): i32

    /// `psignal(sig, msg)` — print a description of `sig` to stderr.
    @lib @optional pub fn psignal(sig: i32, msg: *const char)

    /// `strsignal(sig)` — locale-aware human-readable name.
    @lib @optional pub fn strsignal(sig: i32): *const char
}
