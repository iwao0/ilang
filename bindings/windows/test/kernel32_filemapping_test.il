/// kernel32 — anonymous file-mapping shared memory.

use std.test
use windows {
    HANDLE,
    CreateFileMappingA, MapViewOfFile, UnmapViewOfFile, CloseHandle,
    PageProtection,
}

@extern(C) {
    pub fn run() {
        // Pagefile-backed mapping: INVALID_HANDLE_VALUE for hFile.
        // 8 KB region (low DWORD = 8192, high = 0).
        let invalidHandle = -1 as i64 as HANDLE
        let map = CreateFileMappingA(
            invalidHandle,
            0 as *void,
            PageProtection.readWrite as u32,
            0 as u32,
            8192 as u32,
            0 as *const char
        )
        test.expectTrue((map as i64) != 0)

        // FILE_MAP_ALL_ACCESS = 0xF001F, FILE_MAP_READ = 4, WRITE = 2.
        // Use FILE_MAP_ALL_ACCESS so we can write through the pointer.
        let view = MapViewOfFile(
            map,
            0xF001F as u32,
            0 as u32,
            0 as u32,
            0 as u64       // 0 = whole mapping
        )
        test.expectTrue((view as i64) != 0)

        // The first byte should be zero-initialised.
        let p = view as *u8
        let _ = p           // touch the pointer; reading bytes requires
                            // unsafe deref which we skip here.

        test.expectTrue(UnmapViewOfFile(view) != 0)
        test.expectTrue(CloseHandle(map) != 0)
    }
}

run()
