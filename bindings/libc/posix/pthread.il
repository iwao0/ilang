/// POSIX `<pthread.h>` — threads, mutexes, condition variables.
///
/// Layouts:
///   `pthread_t`      — pointer-sized opaque on glibc / musl / macOS
///   `pthread_mutex_t`— 40 bytes on glibc, 64 on musl, 64 on macOS
///   `pthread_cond_t` — 48 bytes on glibc, 48 on musl, 48 on macOS
///   `pthread_rwlock_t` — 56 / 56 / 200 (macOS uses an opaque blob)
///   `pthread_attr_t` — 56 / 56 / 64
///
/// We over-reserve each opaque so a single binding works on every
/// supported host without per-platform layouts.

@extern(C, "c") {
    /// Opaque thread id.
    pub struct pthread_t {
        _opaque: u8[8]
    }

    /// Opaque mutex. 64 bytes covers glibc + musl + macOS.
    pub struct pthread_mutex_t {
        _opaque: u8[64]
    }

    /// Opaque condition variable.
    pub struct pthread_cond_t {
        _opaque: u8[64]
    }

    /// Opaque rwlock — macOS uses ~200 bytes, glibc ~56. Reserve 256.
    pub struct pthread_rwlock_t {
        _opaque: u8[256]
    }

    /// Opaque thread attribute set.
    pub struct pthread_attr_t {
        _opaque: u8[64]
    }

    /// Opaque mutex attribute set.
    pub struct pthread_mutexattr_t {
        _opaque: u8[16]
    }

    /// Opaque condition-variable attribute set.
    pub struct pthread_condattr_t {
        _opaque: u8[16]
    }

    /// Opaque rwlock attribute set.
    pub struct pthread_rwlockattr_t {
        _opaque: u8[16]
    }

    /// Once-control sentinel. PTHREAD_ONCE_INIT == { 0 } on every
    /// host we care about; 32 bytes is enough for the layout.
    pub struct pthread_once_t {
        _opaque: u8[32]
    }

    /// Thread-specific storage key. POSIX `pthread_key_t` is `int`
    /// on glibc + macOS.
    pub struct pthread_key_t {
        _opaque: u8[4]
    }

    // ── Thread lifecycle ────────────────────────────────────

    @lib @optional pub fn pthread_create(
        thread:    *pthread_t,
        attr:      *const pthread_attr_t,
        start:     fn(*void): *void,
        arg:       *void
    ): i32

    @lib @optional pub fn pthread_join(thread: pthread_t, retval: **void): i32
    @lib @optional pub fn pthread_detach(thread: pthread_t): i32
    @lib @optional pub fn pthread_exit(retval: *void)
    @lib @optional pub fn pthread_self(): pthread_t
    @lib @optional pub fn pthread_equal(a: pthread_t, b: pthread_t): i32
    @lib @optional pub fn pthread_cancel(thread: pthread_t): i32
    @lib @optional pub fn pthread_kill(thread: pthread_t, sig: i32): i32

    // ── Mutex ────────────────────────────────────────────────

    @lib @optional pub fn pthread_mutex_init(
        mutex: *pthread_mutex_t,
        attr:  *const pthread_mutexattr_t
    ): i32
    @lib @optional pub fn pthread_mutex_destroy(mutex: *pthread_mutex_t): i32
    @lib @optional pub fn pthread_mutex_lock(mutex: *pthread_mutex_t): i32
    @lib @optional pub fn pthread_mutex_trylock(mutex: *pthread_mutex_t): i32
    @lib @optional pub fn pthread_mutex_unlock(mutex: *pthread_mutex_t): i32
    @lib @optional pub fn pthread_mutex_timedlock(
        mutex:   *pthread_mutex_t,
        abstime: *const void
    ): i32

    @lib @optional pub fn pthread_mutexattr_init(a: *pthread_mutexattr_t): i32
    @lib @optional pub fn pthread_mutexattr_destroy(a: *pthread_mutexattr_t): i32
    @lib @optional pub fn pthread_mutexattr_settype(a: *pthread_mutexattr_t, kind: i32): i32

    // ── Condition variable ──────────────────────────────────

    @lib @optional pub fn pthread_cond_init(
        cond: *pthread_cond_t,
        attr: *const pthread_condattr_t
    ): i32
    @lib @optional pub fn pthread_cond_destroy(cond: *pthread_cond_t): i32
    @lib @optional pub fn pthread_cond_signal(cond: *pthread_cond_t): i32
    @lib @optional pub fn pthread_cond_broadcast(cond: *pthread_cond_t): i32
    @lib @optional pub fn pthread_cond_wait(cond: *pthread_cond_t, mutex: *pthread_mutex_t): i32
    @lib @optional pub fn pthread_cond_timedwait(
        cond:    *pthread_cond_t,
        mutex:   *pthread_mutex_t,
        abstime: *const void
    ): i32

    // ── rwlock ──────────────────────────────────────────────

    @lib @optional pub fn pthread_rwlock_init(
        rw:   *pthread_rwlock_t,
        attr: *const pthread_rwlockattr_t
    ): i32
    @lib @optional pub fn pthread_rwlock_destroy(rw: *pthread_rwlock_t): i32
    @lib @optional pub fn pthread_rwlock_rdlock(rw: *pthread_rwlock_t): i32
    @lib @optional pub fn pthread_rwlock_tryrdlock(rw: *pthread_rwlock_t): i32
    @lib @optional pub fn pthread_rwlock_wrlock(rw: *pthread_rwlock_t): i32
    @lib @optional pub fn pthread_rwlock_trywrlock(rw: *pthread_rwlock_t): i32
    @lib @optional pub fn pthread_rwlock_unlock(rw: *pthread_rwlock_t): i32

    // ── Once / TSS ──────────────────────────────────────────

    @lib @optional pub fn pthread_once(once: *pthread_once_t, init: fn()): i32

    @lib @optional pub fn pthread_key_create(
        key:  *pthread_key_t,
        dtor: fn(*void)
    ): i32
    @lib @optional pub fn pthread_key_delete(key: pthread_key_t): i32
    @lib @optional pub fn pthread_setspecific(key: pthread_key_t, value: *const void): i32
    @lib @optional pub fn pthread_getspecific(key: pthread_key_t): *void

    // ── Attributes ──────────────────────────────────────────

    @lib @optional pub fn pthread_attr_init(attr: *pthread_attr_t): i32
    @lib @optional pub fn pthread_attr_destroy(attr: *pthread_attr_t): i32
    @lib @optional pub fn pthread_attr_setstacksize(attr: *pthread_attr_t, size: size_t): i32
    @lib @optional pub fn pthread_attr_getstacksize(attr: *const pthread_attr_t, size: *size_t): i32
    @lib @optional pub fn pthread_attr_setdetachstate(attr: *pthread_attr_t, state: i32): i32
}
