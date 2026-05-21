/// POSIX `<semaphore.h>` — counting semaphores.
///
/// glibc / musl: `sem_t` is 32 bytes (4 longs).
/// macOS:        unnamed semaphores aren't supported — `sem_init`
///               returns -1 with errno=ENOSYS. Use `sem_open` for
///               a named semaphore instead.
/// We reserve 64 bytes so the binding works on every host that
/// implements the unnamed variant.

@extern(C, "c") {
    /// Opaque counting semaphore.
    pub struct sem_t {
        _opaque: u8[64]
    }

    /// Initialise an *unnamed* semaphore. `pshared`: 0 ≡
    /// thread-shared, non-zero ≡ process-shared.
    @lib @optional pub fn sem_init(sem: *sem_t, pshared: i32, value: u32): i32
    @lib @optional pub fn sem_destroy(sem: *sem_t): i32

    /// Block until the semaphore is positive, then decrement.
    @lib @optional pub fn sem_wait(sem: *sem_t): i32
    @lib @optional pub fn sem_trywait(sem: *sem_t): i32
    @lib @optional pub fn sem_timedwait(sem: *sem_t, abstime: *const void): i32

    /// Increment the semaphore (wake a waiter).
    @lib @optional pub fn sem_post(sem: *sem_t): i32

    /// Read the current count into `*sval`.
    @lib @optional pub fn sem_getvalue(sem: *sem_t, sval: *i32): i32

    // ── Named semaphores ──────────────────────────────────

    /// Open / create a named semaphore. `flags` is a subset of
    /// `OpenFlags` (O_CREAT | O_EXCL). Returns SEM_FAILED
    /// (`-1 as *sem_t`) on failure.
    @lib @optional pub fn sem_open(
        name:  *const char,
        flags: i32,
        mode:  u32,
        value: u32
    ): *sem_t
    @lib @optional pub fn sem_close(sem: *sem_t): i32
    @lib @optional pub fn sem_unlink(name: *const char): i32
}
