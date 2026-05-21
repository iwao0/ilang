/// C11 `<threads.h>` — portable threads.
///
/// Provided by glibc since 2.28 and by ucrtbase on Windows 10
/// 1903+. macOS / clang on older Apple SDKs are missing this
/// header; every symbol below is `@optional` so a host without
/// C11 threads aborts only on actual use.
///
/// Return codes (`thrd_*` family):
///   0 = thrd_success
///   1 = thrd_nomem
///   2 = thrd_timedout
///   3 = thrd_busy
///   4 = thrd_error
///
/// Mutex types (`mtx_init` `type` argument, OR-able):
///   mtx_plain     0x0
///   mtx_recursive 0x1
///   mtx_timed     0x2

pub enum ThrdResult: i32 {
    success  = 0
    nomem    = 1
    timedout = 2
    busy     = 3
    error    = 4
}

@flags
pub enum MtxType: i32 {
    plain     = 0
    recursive = 1
    timed     = 2
}

@extern(C, "c") {
    /// Opaque thread identifier. glibc treats `thrd_t` as
    /// `pthread_t` (pointer-sized); ucrt uses a 12-byte struct.
    /// 16 bytes covers both, aligned to 8.
    pub struct thrd_t {
        _opaque: u8[16]
    }

    /// Opaque mutex. glibc `mtx_t` is 40 bytes; ucrt is 80. We
    /// reserve 96 to be safe and aligned.
    pub struct mtx_t {
        _opaque: u8[96]
    }

    /// Opaque condition variable. glibc: 48 bytes; ucrt: 72.
    pub struct cnd_t {
        _opaque: u8[96]
    }

    /// Thread-specific storage key. Pointer-sized on every host.
    pub struct tss_t {
        _opaque: u8[8]
    }

    /// `once_flag` is an integer-shaped sentinel. 16 bytes covers
    /// every known layout.
    pub struct once_flag {
        _opaque: u8[16]
    }

    // ── Thread lifecycle ─────────────────────────────────────

    /// Start a new thread running `func(arg)`. The thread id is
    /// written to `*thr`. Returns `ThrdResult`.
    @lib @optional pub fn thrd_create(
        thr:  *thrd_t,
        func: fn(*void): i32,
        arg:  *void
    ): i32

    /// Wait for `thr` to terminate; the function's return value
    /// is written to `*res` when non-NULL.
    @lib @optional pub fn thrd_join(thr: thrd_t, res: *i32): i32

    /// Release thread resources without waiting.
    @lib @optional pub fn thrd_detach(thr: thrd_t): i32

    /// Terminate the calling thread with result `res`. No return.
    @lib @optional pub fn thrd_exit(res: i32)

    /// Sleep at least `*duration`. `remaining` (may be NULL)
    /// receives the unslept remainder if interrupted.
    @lib @optional pub fn thrd_sleep(duration: *const void, remaining: *void): i32

    /// Yield to the scheduler.
    @lib @optional pub fn thrd_yield()

    /// Compare two thread ids — non-zero when they refer to the
    /// same thread.
    @lib @optional pub fn thrd_equal(a: thrd_t, b: thrd_t): i32

    /// The calling thread's id.
    @lib @optional pub fn thrd_current(): thrd_t

    // ── Mutex ─────────────────────────────────────────────────

    /// `type` is a bitwise-or of `MtxType` values.
    @lib @optional pub fn mtx_init(mtx: *mtx_t, type_: i32): i32
    @lib @optional pub fn mtx_lock(mtx: *mtx_t): i32
    @lib @optional pub fn mtx_trylock(mtx: *mtx_t): i32
    @lib @optional pub fn mtx_timedlock(mtx: *mtx_t, ts: *const void): i32
    @lib @optional pub fn mtx_unlock(mtx: *mtx_t): i32
    @lib @optional pub fn mtx_destroy(mtx: *mtx_t)

    // ── Condition variable ────────────────────────────────────

    @lib @optional pub fn cnd_init(cnd: *cnd_t): i32
    @lib @optional pub fn cnd_signal(cnd: *cnd_t): i32
    @lib @optional pub fn cnd_broadcast(cnd: *cnd_t): i32
    @lib @optional pub fn cnd_wait(cnd: *cnd_t, mtx: *mtx_t): i32
    @lib @optional pub fn cnd_timedwait(cnd: *cnd_t, mtx: *mtx_t, ts: *const void): i32
    @lib @optional pub fn cnd_destroy(cnd: *cnd_t)

    // ── Thread-specific storage ──────────────────────────────

    @lib @optional pub fn tss_create(key: *tss_t, dtor: fn(*void)): i32
    @lib @optional pub fn tss_get(key: tss_t): *void
    @lib @optional pub fn tss_set(key: tss_t, value: *void): i32
    @lib @optional pub fn tss_delete(key: tss_t)

    // ── call_once ─────────────────────────────────────────────

    /// Run `func` exactly once per `flag` value across all threads.
    @lib @optional pub fn call_once(flag: *once_flag, func: fn())
}
