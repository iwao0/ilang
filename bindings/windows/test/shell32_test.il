/// shell32 — special folder lookup.

use std.test
use windows {
    SHGetFolderPathA,
    CSIDL,
    HeapAlloc, HeapFree, GetProcessHeap,
    lstrlenA,
}

@extern(C) {
    pub fn run() {
        let heap = GetProcessHeap()
        // MAX_PATH = 260 bytes (one extra for NUL).
        let buf = HeapAlloc(heap, 8 as u32, 261 as u64)

        // CSIDL_WINDOWS = 0x0024 → usually "C:\\Windows".
        let rc = SHGetFolderPathA(
            0 as i64,
            CSIDL.windows as i32,
            0 as i64,
            0 as u32,
            buf as *char
        )
        test.expect(rc as i64, 0)
        let len = lstrlenA(buf as *const char)
        test.expectTrue(len > 0)
        test.expectTrue(len < 260)

        // CSIDL_PROFILE = 0x0028 → user profile dir.
        let rc2 = SHGetFolderPathA(
            0 as i64,
            CSIDL.profile as i32,
            0 as i64,
            0 as u32,
            buf as *char
        )
        test.expect(rc2 as i64, 0)
        test.expectTrue(lstrlenA(buf as *const char) > 0)

        HeapFree(heap, 0 as u32, buf)
    }
}

run()
