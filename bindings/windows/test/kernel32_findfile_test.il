/// kernel32 — file enumeration over the Windows system directory.

use test
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
            _fn00: 0, _fn01: 0, _fn02: 0, _fn03: 0, _fn04: 0, _fn05: 0,
            _fn06: 0, _fn07: 0, _fn08: 0, _fn09: 0, _fn10: 0, _fn11: 0,
            _fn12: 0, _fn13: 0, _fn14: 0, _fn15: 0, _fn16: 0, _fn17: 0,
            _fn18: 0, _fn19: 0, _fn20: 0, _fn21: 0, _fn22: 0, _fn23: 0,
            _fn24: 0, _fn25: 0, _fn26: 0, _fn27: 0, _fn28: 0, _fn29: 0,
            _fn30: 0, _fn31: 0, _fn32: 0, _fn33: 0, _fn34: 0, _fn35: 0,
            _fn36: 0, _fn37: 0, _fn38: 0, _fn39: 0, _fn40: 0, _fn41: 0,
            _fn42: 0, _fn43: 0, _fn44: 0, _fn45: 0, _fn46: 0, _fn47: 0,
            _fn48: 0, _fn49: 0, _fn50: 0, _fn51: 0, _fn52: 0, _fn53: 0,
            _fn54: 0, _fn55: 0, _fn56: 0, _fn57: 0, _fn58: 0, _fn59: 0,
            _fn60: 0, _fn61: 0, _fn62: 0, _fn63: 0, _fn64: 0,
            _af0: 0, _af1: 0, _af2: 0, _af3: 0,
        }
        let h = FindFirstFileA(pat, fd)
        // INVALID_HANDLE_VALUE has bit pattern 0xFFFFFFFFFFFFFFFF.
        test.expectTrue((h as i64) != -1)

        // First entry has a non-zero file size attribute set.
        test.expectTrue(fd.dwFileAttributes != 0)

        // Count files (cap to avoid infinite loops on bad implementations).
        let count = 1
        let max = 1000
        while count < max {
            let more = FindNextFileA(h, fd)
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
