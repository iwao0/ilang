/// kernel32 — file enumeration over the Windows system directory.

use std.ffi { cstrFromString, freeCstr }
use std.test as test
use windows {
    WIN32_FIND_DATAA,
    FindFirstFileA, FindNextFileA, FindClose,
}

@extern(C) {
    pub fn run() {
        // Iterate over C:\\Windows\\System32\\*.dll — there are always
        // hundreds of matches.
        let pat = cstrFromString("C:\\Windows\\System32\\*.dll")
        let fd = WIN32_FIND_DATAA {
            dwFileAttributes: 0,
            ftCreationTime_lo: 0, ftCreationTime_hi: 0,
            ftLastAccessTime_lo: 0, ftLastAccessTime_hi: 0,
            ftLastWriteTime_lo: 0, ftLastWriteTime_hi: 0,
            nFileSizeHigh: 0, nFileSizeLow: 0,
            dwReserved0: 0, dwReserved1: 0,
            cFileName: [
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0
            ],
            cAlternateFileName: [
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ],
            _pad: 0,
        }
        let h = FindFirstFileA(pat, &fd)
        // INVALID_HANDLE_VALUE has bit pattern 0xFFFFFFFFFFFFFFFF.
        test.expectTrue((h as i64) != -1)

        // First entry has a non-zero file size attribute set.
        test.expectTrue(fd.dwFileAttributes != 0)

        // Count files (cap to avoid infinite loops on bad implementations).
        let count = 1
        let max = 1000
        while count < max {
            let more = FindNextFileA(h, &fd)
            if more == 0 {
                break
            }
            count = count + 1
        }
        // System32 always has way more than 50 DLLs.
        test.expectTrue(count > 50)

        test.expectTrue(FindClose(h) != 0)
        freeCstr(pat)
    }
}

run()
