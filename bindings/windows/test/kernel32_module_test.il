/// kernel32 — module / executable path queries.

use test
use windows {
    HMODULE, HANDLE,
    GetModuleHandleA, GetModuleFileNameA, GetCommandLineA,
    GetWindowsDirectoryA, GetSystemDirectoryA, GetSystemWindowsDirectoryA,
    GetUserNameA,
    GetCurrentProcess,
    HeapAlloc, HeapFree, GetProcessHeap,
    lstrlenA,
}

@extern(C) {
    pub fn run() {
        let heap = GetProcessHeap()
        let buf = HeapAlloc(heap, 8 as u32, 1024 as u64)

        // GetModuleFileNameA for the current process — returns path > 5.
        let exeName = GetModuleFileNameA(0 as HMODULE, buf as *char, 1024 as u32)
        test.expectTrue(exeName > 5)

        // GetCommandLineA returns a static string; non-empty.
        let cmd = GetCommandLineA()
        test.expectTrue(lstrlenA(cmd) > 0)

        // System dirs — non-empty.
        test.expectTrue(GetSystemDirectoryA(buf as *char, 1024 as u32) > 5)
        test.expectTrue(GetWindowsDirectoryA(buf as *char, 1024 as u32) > 5)
        test.expectTrue(GetSystemWindowsDirectoryA(buf as *char, 1024 as u32) > 5)

        // GetUserNameA — name buffer + length param.
        let len: u32 = 1024
        test.expectTrue(GetUserNameA(buf as *char, &len) != 0)
        test.expectTrue(len > 0)

        // GetModuleHandleA("kernel32.dll") returns the kernel32 base.
        let k32 = cstrFromString("kernel32.dll")
        let hk = GetModuleHandleA(k32)
        test.expectTrue((hk as i64) != 0)
        freeCstr(k32)

        HeapFree(heap, 0 as u32, buf)
    }
}

run()
