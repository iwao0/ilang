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

// byAppendingPathExtension and byDeletingPathExtension.
let withExt = NSURL.fromFilePath(nsstr("/tmp/foo"))
    .byAppendingPathExtension(nsstr("json"))
test.expectStr(ilstr(withExt.pathExtension), "json")
test.expectStr(
    ilstr(withExt.byDeletingPathExtension.lastPathComponent),
    "foo"
)

// pathComponents on a file URL.
let pc = NSURL.fromFilePath(nsstr("/a/b/c")).pathComponents
test.expectTrue(pc.count >= 4)  // [/, a, b, c]

// hasDirectoryPath.
test.expectFalse(NSURL.fromFilePath(nsstr("/tmp/foo.txt")).hasDirectoryPath)

// standardizedURL on a URL with `..` collapses it.
let std = NSURL.fromFilePath(nsstr("/a/b/../c")).standardizedURL
test.expectStr(ilstr(std.path), "/a/c")

// absoluteURL of an already-absolute URL is itself.
let abs = NSURL.fromString(nsstr("https://example.com/x"))
test.expectStr(ilstr(abs.absoluteURL.absoluteString), "https://example.com/x")

// byStandardizingPath on a file URL.
let trailing = NSURL.fromFilePath(nsstr("/tmp/."))
test.expectStr(ilstr(trailing.byStandardizingPath.path), "/tmp")

// byAppendingPathComponentDir (with the trailing-slash hint).
let dir = NSURL.fromFilePath(nsstr("/tmp")).byAppendingPathComponentDir(
    nsstr("data"), true
)
test.expectStr(ilstr(dir.lastPathComponent), "data")
test.expectTrue(dir.hasDirectoryPath)

// pathComponents on a non-file URL.
let urlComps = NSURL.fromString(nsstr("https://a.com/b/c/d")).pathComponents
test.expectTrue(urlComps.count >= 3)

// relativePath / relativeString on a plain URL == path / absolute.
let plain = NSURL.fromString(nsstr("https://x.com/y"))
test.expectStr(ilstr(plain.relativePath), "/y")
test.expectStr(ilstr(plain.relativeString), "https://x.com/y")

// baseURL on an absolute URL is the nil-handle wrapper.
test.expectTrue(plain.baseURL.handle == 0)

0
