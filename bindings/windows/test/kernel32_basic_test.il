/// kernel32 — basic process / module / time queries.

use std.test as test
use windows {
    GetTickCount, GetTickCount64,
    GetCurrentProcessId, GetCurrentThreadId,
    GetCurrentProcess, GetCurrentThread,
    GetModuleHandleA,
    QueryPerformanceCounter, QueryPerformanceFrequency,
    GetLastError, SetLastError,
}

@extern(C) {
    pub fn run() {
        // GetTickCount should return a non-zero value (system has been
        // up since boot).
        let t32 = GetTickCount()
        test.expectTrue(t32 > 0)

        let t64 = GetTickCount64()
        test.expectTrue((t64 as i64) > 0)

        // Process / thread IDs are non-zero for the current process.
        let pid = GetCurrentProcessId()
        let tid = GetCurrentThreadId()
        test.expectTrue(pid > 0)
        test.expectTrue(tid > 0)

        // GetCurrentProcess / GetCurrentThread return pseudo-handles
        // (constants -1 and -2 respectively, but their bit pattern
        // is non-null).
        let hp = GetCurrentProcess()
        let ht = GetCurrentThread()
        test.expectTrue((hp as i64) != 0)
        test.expectTrue((ht as i64) != 0)

        // GetModuleHandleA(NULL) returns the EXE's HMODULE.
        let exe = GetModuleHandleA(0 as *const char)
        test.expectTrue((exe as i64) != 0)

        // QueryPerformanceFrequency/Counter — high-res timer.
        let freq: i64 = 0
        let ok_f = QueryPerformanceFrequency(&freq)
        test.expectTrue(ok_f != 0)
        test.expectTrue(freq > 0)

        let counter: i64 = 0
        let ok_c = QueryPerformanceCounter(&counter)
        test.expectTrue(ok_c != 0)
        test.expectTrue(counter > 0)

        // GetLastError / SetLastError round-trip.
        SetLastError(12345)
        test.expect(GetLastError() as i64, 12345)
        SetLastError(0)
        test.expect(GetLastError() as i64, 0)
    }
}

run()
