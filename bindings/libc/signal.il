/// C standard `<signal.h>` — async / sync signal handling.
///
/// Standard `SIG*` numbers (POSIX defaults; Windows ucrt uses the
/// same values for the subset it supports):
///
///   SIGINT   2   Ctrl-C
///   SIGILL   4   Illegal instruction
///   SIGABRT  6   Raised by `abort()`
///   SIGFPE   8   Floating-point exception
///   SIGSEGV 11   Segmentation fault
///   SIGTERM 15   Polite termination request
///
/// POSIX-only (not on Windows):
///   SIGHUP    1   SIGPIPE 13   SIGALRM 14
///   SIGUSR1  10   SIGUSR2 12   SIGCHLD 17
///
/// Special handler values for `signal`:
///   SIG_DFL   = 0 as fn(i32)   — default action
///   SIG_IGN   = 1 as fn(i32)   — ignore the signal
///   SIG_ERR   = -1 as fn(i32)  — returned on error

@extern(C, "c") {
    /// Install `handler` as the action for signal `signum`. Returns
    /// the previous handler, or SIG_ERR (-1) on failure.
    @lib pub fn signal(signum: i32, handler: fn(i32)): fn(i32)

    /// Send `sig` to the calling process. Returns 0 on success.
    @lib pub fn raise(sig: i32): i32
}
