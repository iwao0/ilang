// NSFileManager — read-only filesystem checks against paths the
// macOS host is guaranteed to have. No writes / mutations.

use test
use foundation { NSFileManager, nsstr, ilstr }

let fm = NSFileManager.defaultManager()

// `/` is always present on macOS.
test.expectTrue(fm.fileExistsAtPath(nsstr("/")))
test.expectTrue(fm.fileExistsAtPath(nsstr("/usr")))
test.expectTrue(fm.fileExistsAtPath(nsstr("/usr/bin")))

// Sentinel non-existent path.
test.expectFalse(fm.fileExistsAtPath(
    nsstr("/__ilang_nonexistent_path__/should/not/exist")
))

// currentDirectoryPath returns a non-empty string.
let cwd = fm.currentDirectoryPath
test.expectTrue(cwd.length > 0)

// contentsOfDirectoryAtPath on /usr always has bin / lib / share.
let usr = fm.contentsOfDirectoryAtPath(nsstr("/usr"), 0)
test.expectTrue(usr.count > 0)

0
