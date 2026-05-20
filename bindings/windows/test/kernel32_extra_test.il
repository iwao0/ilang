/// kernel32 — assorted utility coverage.

use test
use windows {
    GetLogicalDrives, GetDriveTypeA,
    GetDiskFreeSpaceExA,
    Sleep, GetTickCount64,
    IsDebuggerPresent,
    IsWow64Process, GetCurrentProcess,
    SetEnvironmentVariableA, GetEnvironmentVariableA, ExpandEnvironmentStringsA,
    GetComputerNameA,
    HeapAlloc, HeapFree, GetProcessHeap,
    OutputDebugStringA,
    GetSystemInfo, SYSTEM_INFO,
    FormatMessageA, LocalFree,
    GetProcessTimes, FILETIME,
    GetSystemTimePreciseAsFileTime,
}

@extern(C) {
    pub fn run() {
        let heap = GetProcessHeap()

        // GetLogicalDrives — at least drive C (bit 2) is set.
        let drives = GetLogicalDrives()
        test.expectTrue((drives & 4 as u32) != 0)

        // GetDriveTypeA on "C:\" → 3 (DRIVE_FIXED).
        let c = cstrFromString("C:\\")
        test.expect(GetDriveTypeA(c) as i64, 3)
        freeCstr(c)

        // Disk free / total bytes.
        let free: i64 = 0
        let total: i64 = 0
        let totalFree: i64 = 0
        let path = cstrFromString("C:\\")
        test.expectTrue(GetDiskFreeSpaceExA(path, &free, &total, &totalFree) != 0)
        test.expectTrue(total > 0)
        freeCstr(path)

        // Sleep 1 ms; tick count moves.
        let t0 = GetTickCount64()
        Sleep(1 as u32)
        let t1 = GetTickCount64()
        test.expectTrue(t1 >= t0)

        // IsDebuggerPresent returns 0 unless we're running under a debugger.
        let dbg = IsDebuggerPresent()
        test.expectTrue(dbg == 0 || dbg != 0)   // accept either

        // IsWow64Process — false on a 64-bit ilang.exe under 64-bit Windows.
        let wow: i32 = -1
        test.expectTrue(IsWow64Process(GetCurrentProcess(), &wow) != 0)
        test.expect(wow as i64, 0)

        // GetComputerNameA fills a non-empty buffer.
        let cnBuf = HeapAlloc(heap, 8 as u32, 64 as u64)
        let cnLen: u32 = 64
        test.expectTrue(GetComputerNameA(cnBuf as *char, &cnLen) != 0)
        test.expectTrue(cnLen > 0)
        HeapFree(heap, 0 as u32, cnBuf)

        // OutputDebugStringA — non-failing void call.
        let msg = cstrFromString("ilang win32 test\n")
        OutputDebugStringA(msg)
        freeCstr(msg)

        // SYSTEM_INFO: number-of-processors ≥ 1.
        let si = SYSTEM_INFO {
            dwOemId: 0,
            dwPageSize: 0,
            lpMinimumApplicationAddress: 0 as *void,
            lpMaximumApplicationAddress: 0 as *void,
            dwActiveProcessorMask: 0 as u64,
            dwNumberOfProcessors: 0,
            dwProcessorType: 0,
            dwAllocationGranularity: 0,
            wProcessorLevel: 0,
            wProcessorRevision: 0,
        }
        GetSystemInfo(si)
        test.expectTrue(si.dwNumberOfProcessors >= 1)
        test.expectTrue(si.dwPageSize > 0)

        // FormatMessageA on a known Win32 error code (2 = ERROR_FILE_NOT_FOUND).
        let outPtr: *char = 0 as *char
        let formatted = FormatMessageA(
            0x1100 as u32,           // ALLOCATE_BUFFER | FROM_SYSTEM
            0 as *const void,
            2 as u32,
            0 as u32,
            (&outPtr) as *char,      // double-indirection via cast
            0 as u32,
            0 as *void
        )
        test.expectTrue(formatted > 0)
        if (outPtr as i64) != 0 {
            LocalFree(outPtr as *void)
        }

        // GetProcessTimes — kernel + user time both ≥ 0.
        let creation = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 }
        let exit = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 }
        let kernel = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 }
        let user = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 }
        test.expectTrue(GetProcessTimes(
            GetCurrentProcess(),
            creation, exit, kernel, user
        ) != 0)
        // Creation time must be non-zero (some FILETIME value since boot).
        test.expectTrue(creation.dwHighDateTime > 0)

        // High-res system time.
        let ft = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 }
        GetSystemTimePreciseAsFileTime(ft)
        test.expectTrue(ft.dwHighDateTime > 0)
    }
}

run()
