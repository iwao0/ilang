/// shlwapi — path inspection / decomposition.

use std.test
use shlwapi {
use std.ffi { cstrFromString, freeCstr, stringFromCstr }
    PathFileExistsA,
    PathIsRelativeA,
    PathIsRootA,
    PathFindFileNameA,
    PathFindExtensionA,
    PathGetDriveNumberA,
}

@extern(C) {
    pub fn run() {
        // PathFileExistsA on a path that should always exist on Windows.
        let sysroot = cstrFromString("C:\\Windows")
        test.expectTrue(PathFileExistsA(sysroot) != 0)
        freeCstr(sysroot)

        let missing = cstrFromString("C:\\__definitely_does_not_exist__")
        test.expect(PathFileExistsA(missing) as i64, 0)
        freeCstr(missing)

        // PathIsRelativeA.
        let abs = cstrFromString("C:\\foo\\bar.txt")
        test.expect(PathIsRelativeA(abs) as i64, 0)
        freeCstr(abs)

        let rel = cstrFromString("foo\\bar.txt")
        test.expectTrue(PathIsRelativeA(rel) != 0)
        freeCstr(rel)

        // PathIsRootA — C:\ is a root.
        let root = cstrFromString("C:\\")
        test.expectTrue(PathIsRootA(root) != 0)
        freeCstr(root)

        let notroot = cstrFromString("C:\\foo")
        test.expect(PathIsRootA(notroot) as i64, 0)
        freeCstr(notroot)

        // PathFindFileNameA — returns pointer to start of filename.
        let p = cstrFromString("C:\\dir\\sub\\file.txt")
        let name = PathFindFileNameA(p)
        test.expectStr(stringFromCstr(name), "file.txt")
        freeCstr(p)

        // PathFindExtensionA — returns pointer to ".txt".
        let p2 = cstrFromString("C:\\foo\\bar.txt")
        let ext = PathFindExtensionA(p2)
        test.expectStr(stringFromCstr(ext), ".txt")
        freeCstr(p2)

        // No extension → returns pointer to NUL terminator (empty string).
        let p3 = cstrFromString("C:\\foo\\bar")
        let ext3 = PathFindExtensionA(p3)
        test.expectStr(stringFromCstr(ext3), "")
        freeCstr(p3)

        // Drive number: C: → 2, D: → 3, etc.
        let drv = cstrFromString("C:\\foo")
        test.expect(PathGetDriveNumberA(drv) as i64, 2)
        freeCstr(drv)
    }
}

run()
