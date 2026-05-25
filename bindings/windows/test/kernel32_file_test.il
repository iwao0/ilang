/// kernel32 — file create / write / read / delete round-trip.

use std.ffi { cstrFromString, freeCstr, stringFromCstr }
use std.test as test
use windows {
    HANDLE,
    CreateFileA, ReadFile, WriteFile, CloseHandle,
    DeleteFileA, GetFileSizeEx, SetFilePointerEx,
    GetTempPathA, GetTempFileNameA,
    HeapAlloc, HeapFree, GetProcessHeap,
    FileAccess, FileShare, FileCreation, FileAttribute,
    lstrlenA,
}

@extern(C) {
    pub fn run() {
        let heap = GetProcessHeap()

        // Build a unique temp filename via GetTempPathA + GetTempFileNameA.
        let tmpPath = HeapAlloc(heap, 8 as u32, 1024 as u64)
        let tpLen = GetTempPathA(1024 as u32, tmpPath as *char)
        test.expectTrue(tpLen > 0)

        let prefix = cstrFromString("il_")
        let tmpFile = HeapAlloc(heap, 8 as u32, 1024 as u64)
        let unique = GetTempFileNameA(
            tmpPath as *const char,
            prefix,
            0 as u32,
            tmpFile as *char
        )
        test.expectTrue(unique != 0)
        freeCstr(prefix)

        // Open for write.
        let h = CreateFileA(
            tmpFile as *const char,
            FileAccess.write as u32,
            0 as u32,
            0 as *void,
            FileCreation.createAlways as u32,
            FileAttribute.normal as u32,
            0 as HANDLE
        )
        test.expectTrue((h as i64) != 0xFFFFFFFFFFFFFFFF)

        // Write 13 bytes "hello, world!".
        let data = cstrFromString("hello, world!")
        let written: u32 = 0
        let ok = WriteFile(
            h,
            data as *const void,
            13 as u32,
            &written,
            0 as *void
        )
        test.expectTrue(ok != 0)
        test.expect(written as i64, 13)
        CloseHandle(h)
        freeCstr(data)

        // Open for read and read back.
        let h2 = CreateFileA(
            tmpFile as *const char,
            FileAccess.read as u32,
            FileShare.read as u32,
            0 as *void,
            FileCreation.openExisting as u32,
            FileAttribute.normal as u32,
            0 as HANDLE
        )
        test.expectTrue((h2 as i64) != 0xFFFFFFFFFFFFFFFF)

        let size: i64 = 0
        test.expectTrue(GetFileSizeEx(h2, &size) != 0)
        test.expect(size, 13)

        let readBuf = HeapAlloc(heap, 8 as u32, 64 as u64)
        let nread: u32 = 0
        let ok2 = ReadFile(
            h2,
            readBuf,
            13 as u32,
            &nread,
            0 as *void
        )
        test.expectTrue(ok2 != 0)
        test.expect(nread as i64, 13)
        // Ensure NUL terminator for stringFromCstr.
        let zero = HeapAlloc(heap, 8 as u32, 14 as u64)
        // Easier: just compare via lstrlenA after appending NUL ourselves —
        // skip that and instead test the byte count above.
        test.expectStr(stringFromCstr(readBuf as *const char), "hello, world!")

        CloseHandle(h2)
        HeapFree(heap, 0 as u32, readBuf)
        HeapFree(heap, 0 as u32, zero)

        // Clean up the temp file.
        test.expectTrue(DeleteFileA(tmpFile as *const char) != 0)

        HeapFree(heap, 0 as u32, tmpPath)
        HeapFree(heap, 0 as u32, tmpFile)
    }
}

run()
