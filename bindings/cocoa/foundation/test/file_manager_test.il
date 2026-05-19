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

// attributesOfItemAtPath — returns a dictionary with NSFileType
// among other keys for `/`.
let attrs = fm.attributesOfItemAtPath(nsstr("/"), 0)
test.expectTrue(attrs.count > 0)

// changeCurrentDirectoryPath: round-trip to /tmp and back.
let original = fm.currentDirectoryPath
test.expectTrue(fm.changeCurrentDirectoryPath(nsstr("/tmp")))
// macOS canonicalises /tmp → /private/tmp, so check via the
// realpath comparison rather than exact equality.
let nowAt = fm.currentDirectoryPath
test.expectTrue(nowAt.containsString(nsstr("/tmp")))
// Restore.
test.expectTrue(fm.changeCurrentDirectoryPath(original))

0
