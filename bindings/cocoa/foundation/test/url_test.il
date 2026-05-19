// NSURL — construct, inspect components.

use test
use foundation { NSURL, nsstr, ilstr }

let u = NSURL.fromString(nsstr("https://example.com:8080/path/to/x?q=1#frag"))

test.expectStr(ilstr(u.absoluteString), "https://example.com:8080/path/to/x?q=1#frag")
test.expectStr(ilstr(u.scheme), "https")
test.expectStr(ilstr(u.host), "example.com")
test.expectStr(ilstr(u.path), "/path/to/x")
test.expectStr(ilstr(u.query), "q=1")
test.expectStr(ilstr(u.fragment), "frag")
test.expectStr(ilstr(u.lastPathComponent), "x")

// File URL flavour.
let f = NSURL.fromFilePath(nsstr("/tmp/foo.txt"))
test.expectTrue(f.isFileURL)
test.expectStr(ilstr(f.lastPathComponent), "foo.txt")
test.expectStr(ilstr(f.pathExtension), "txt")

// Path-component append / delete.
let appended = f.byDeletingLastPathComponent.byAppendingPathComponent(nsstr("bar.md"))
test.expectStr(ilstr(appended.lastPathComponent), "bar.md")

0
