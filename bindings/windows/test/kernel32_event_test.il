/// kernel32 — Event / mutex synchronisation primitives.

use std.test
use windows {
    HANDLE,
    CreateEventA, SetEvent, ResetEvent,
    CreateMutexA, ReleaseMutex,
    CreateSemaphoreA, ReleaseSemaphore,
    WaitForSingleObject, CloseHandle,
    WaitResult,
}

@extern(C) {
    pub fn run() {
        // Manual-reset event, initially non-signalled.
        let ev = CreateEventA(
            0 as *void,
            1 as i32,       // bManualReset = true
            0 as i32,       // bInitialState = false
            0 as *const char
        )
        test.expectTrue((ev as i64) != 0)

        // Non-signalled: WaitForSingleObject with 0 ms should timeout.
        test.expect(
            WaitForSingleObject(ev, 0 as u32) as i64,
            WaitResult.timeout as i64
        )

        // Signal → wait returns immediately with object0.
        test.expectTrue(SetEvent(ev) != 0)
        test.expect(
            WaitForSingleObject(ev, 0 as u32) as i64,
            WaitResult.object0 as i64
        )
        // Manual-reset stays signalled.
        test.expect(
            WaitForSingleObject(ev, 0 as u32) as i64,
            WaitResult.object0 as i64
        )

        // Reset → signal again to verify the reset works.
        test.expectTrue(ResetEvent(ev) != 0)
        test.expect(
            WaitForSingleObject(ev, 0 as u32) as i64,
            WaitResult.timeout as i64
        )

        test.expectTrue(CloseHandle(ev) != 0)

        // Mutex: initial owner = false → first waiter gets it.
        let mtx = CreateMutexA(0 as *void, 0 as i32, 0 as *const char)
        test.expectTrue((mtx as i64) != 0)
        test.expect(
            WaitForSingleObject(mtx, 0 as u32) as i64,
            WaitResult.object0 as i64
        )
        test.expectTrue(ReleaseMutex(mtx) != 0)
        test.expectTrue(CloseHandle(mtx) != 0)

        // Semaphore initial=1, max=2.
        let sem = CreateSemaphoreA(
            0 as *void,
            1 as i32,
            2 as i32,
            0 as *const char
        )
        test.expectTrue((sem as i64) != 0)
        // Consume the initial token.
        test.expect(
            WaitForSingleObject(sem, 0 as u32) as i64,
            WaitResult.object0 as i64
        )
        // Now empty → timeout.
        test.expect(
            WaitForSingleObject(sem, 0 as u32) as i64,
            WaitResult.timeout as i64
        )
        // Release adds back.
        let prev: i32 = -1
        test.expectTrue(ReleaseSemaphore(sem, 1 as i32, &prev) != 0)
        test.expect(prev as i64, 0)   // count was 0 before release
        test.expectTrue(CloseHandle(sem) != 0)
    }
}

run()
