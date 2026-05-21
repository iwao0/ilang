/// kernel32 — synchronization primitives lifecycle.

use test
use windows {
    CRITICAL_SECTION,
    InitializeCriticalSection, EnterCriticalSection,
    LeaveCriticalSection, TryEnterCriticalSection,
    DeleteCriticalSection,
    InitializeCriticalSectionAndSpinCount,
    SRWLOCK,
    InitializeSRWLock,
    AcquireSRWLockExclusive, ReleaseSRWLockExclusive,
    AcquireSRWLockShared, ReleaseSRWLockShared,
    TryAcquireSRWLockExclusive, TryAcquireSRWLockShared,
}

@extern(C) {
    pub fn run() {
        // Critical section: basic initialise / enter / leave / delete.
        let cs = CRITICAL_SECTION {
            DebugInfo: 0 as *void,
            LockCount: 0,
            RecursionCount: 0,
            OwningThread: 0 as *void,
            LockSemaphore: 0 as *void,
            SpinCount: 0 as u64,
        }
        InitializeCriticalSection(&cs)
        EnterCriticalSection(&cs)
        // Recursive entry is allowed for the owning thread.
        EnterCriticalSection(&cs)
        LeaveCriticalSection(&cs)
        LeaveCriticalSection(&cs)
        // TryEnter returns non-zero on success.
        let got = TryEnterCriticalSection(&cs)
        test.expectTrue(got != 0)
        LeaveCriticalSection(&cs)
        DeleteCriticalSection(&cs)

        // InitializeCriticalSectionAndSpinCount.
        let cs2 = CRITICAL_SECTION {
            DebugInfo: 0 as *void,
            LockCount: 0,
            RecursionCount: 0,
            OwningThread: 0 as *void,
            LockSemaphore: 0 as *void,
            SpinCount: 0 as u64,
        }
        let ok2 = InitializeCriticalSectionAndSpinCount(&cs2, 4000)
        test.expectTrue(ok2 != 0)
        DeleteCriticalSection(&cs2)

        // SRWLOCK: basic acquire/release in both modes.
        let rw = SRWLOCK { Ptr: 0 as *void }
        InitializeSRWLock(&rw)
        AcquireSRWLockExclusive(&rw)
        ReleaseSRWLockExclusive(&rw)
        AcquireSRWLockShared(&rw)
        ReleaseSRWLockShared(&rw)
        // Try-acquire variants.
        let te = TryAcquireSRWLockExclusive(&rw)
        test.expectTrue(te != 0)
        ReleaseSRWLockExclusive(&rw)
        let ts = TryAcquireSRWLockShared(&rw)
        test.expectTrue(ts != 0)
        ReleaseSRWLockShared(&rw)
    }
}

run()
