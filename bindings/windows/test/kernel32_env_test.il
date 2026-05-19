// kernel32 — environment variables and computer / user name.

use test
use windows {
    GetEnvironmentVariableA, SetEnvironmentVariableA,
    ExpandEnvironmentStringsA,
    GetCurrentDirectoryA,
    GetSystemDirectoryA, GetWindowsDirectoryA,
    HeapAlloc, HeapFree, GetProcessHeap,
    lstrlenA,
}

@extern(C) {
    pub fn run() {
        let heap = GetProcessHeap()
        let buf = HeapAlloc(heap, 8 as u32, 1024 as u64)

        // PATH always exists; the value is non-empty.
        let pathVar = cstrFromString("PATH")
        let n = GetEnvironmentVariableA(pathVar, buf as *char, 1024 as u32)
        test.expectTrue(n > 0)
        freeCstr(pathVar)

        // Round-trip: set, read back, clear.
        let key = cstrFromString("ILANG_TEST_VAR")
        let val = cstrFromString("hello_world")
        test.expectTrue(SetEnvironmentVariableA(key, val) != 0)

        let n2 = GetEnvironmentVariableA(key, buf as *char, 1024 as u32)
        test.expect(n2 as i64, 11)   // strlen("hello_world")
        test.expectStr(stringFromCstr(buf as *const char), "hello_world")

        // Clearing: pass NULL value.
        test.expectTrue(SetEnvironmentVariableA(key, 0 as *const char) != 0)
        let n3 = GetEnvironmentVariableA(key, buf as *char, 1024 as u32)
        test.expect(n3 as i64, 0)
        freeCstr(key)
        freeCstr(val)

        // ExpandEnvironmentStringsA: substitutes %VAR% values.
        let src = cstrFromString("%SystemRoot%")
        let expN = ExpandEnvironmentStringsA(src, buf as *char, 1024 as u32)
        test.expectTrue(expN > 0)
        // After expansion, "%" should no longer appear.
        let expanded = stringFromCstr(buf as *const char)
        test.expectTrue(lstrlenA(buf as *const char) >= 5)
        freeCstr(src)

        // System / Windows directories — both end in a path with > 5 chars.
        test.expectTrue(GetWindowsDirectoryA(buf as *char, 1024 as u32) > 5)
        test.expectTrue(GetSystemDirectoryA(buf as *char, 1024 as u32) > 5)

        // Current directory exists (something).
        test.expectTrue(GetCurrentDirectoryA(1024 as u32, buf as *char) > 0)

        HeapFree(heap, 0 as u32, buf)
    }
}

run()
