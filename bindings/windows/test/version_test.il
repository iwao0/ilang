/// version — version-info resource size query for a system DLL.

use std.test
use windows {
use std.ffi { cstrFromString, freeCstr }
    GetFileVersionInfoSizeA,
}

@extern(C) {
    pub fn run() {
        // kernel32.dll always has a version resource. The size is
        // non-zero on any Windows install.
        let path = cstrFromString("C:\\Windows\\System32\\kernel32.dll")
        let handle: u32 = 0
        let size = GetFileVersionInfoSizeA(path, &handle)
        test.expectTrue(size > 0)
        freeCstr(path)

        // A missing file returns 0 and sets the last error.
        let bad = cstrFromString("C:\\__no_such_file__.dll")
        let h2: u32 = 0
        test.expect(GetFileVersionInfoSizeA(bad, &h2) as i64, 0)
        freeCstr(bad)
    }
}

run()
