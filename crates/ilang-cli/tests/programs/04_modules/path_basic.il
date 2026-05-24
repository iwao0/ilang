// Built-in `path` module — Node.js-inspired POSIX-style helpers
// (separator is always `/`). Covers the public API:
//   sep / delimiter / isAbsolute / basename / dirname / extname /
//   join / normalize / relative / parse / format.
use std.path
use std.test

test.expectStr(path.basename("/foo/bar/baz.txt"), "baz.txt")
test.expectStr(path.basename("/foo/bar/"), "bar")
test.expectStr(path.basename(""), "")
test.expectStr(path.basename("/foo/bar/baz.txt", ".txt"), "baz")
test.expectStr(path.basename("/foo/bar/baz.txt", ".gz"), "baz.txt")

test.expectStr(path.dirname("/foo/bar/baz.txt"), "/foo/bar")
test.expectStr(path.dirname("/foo"), "/")
test.expectStr(path.dirname("/"), "/")
test.expectStr(path.dirname("foo"), ".")
test.expectStr(path.dirname(""), ".")

test.expectStr(path.extname("a.tar.gz"), ".gz")
test.expectStr(path.extname("a"), "")
test.expectStr(path.extname(".bashrc"), "")
test.expectStr(path.extname("foo.txt"), ".txt")

test.expectBool(path.isAbsolute("/foo"), true)
test.expectBool(path.isAbsolute("foo"), false)
test.expectBool(path.isAbsolute(""), false)

test.expectStr(path.join(["a", "b", "c"]), "a/b/c")
test.expectStr(path.join(["/a", "b", "c"]), "/a/b/c")
test.expectStr(path.join(["a", "..", "b"]), "b")
test.expectStr(path.join(["a", ".", "b"]), "a/b")
test.expectStr(path.join([""]), ".")

test.expectStr(path.normalize("/a//b/c/../d"), "/a/b/d")
test.expectStr(path.normalize("a/b/../c"), "a/c")
test.expectStr(path.normalize("./a/./b"), "a/b")
test.expectStr(path.normalize("../a"), "../a")
test.expectStr(path.normalize("/.."), "/")
test.expectStr(path.normalize(""), ".")
test.expectStr(path.normalize("/a/b/"), "/a/b/")

test.expectStr(path.relative("/a/b/c", "/a/b/d"), "../d")
test.expectStr(path.relative("/a/b", "/a/b/c"), "c")
test.expectStr(path.relative("/a/b/c", "/a/b"), "..")
test.expectStr(path.relative("/a/b", "/a/b"), "")

let p = path.parse("/foo/bar/baz.txt")
test.expectStr(p.dir, "/foo/bar")
test.expectStr(p.root, "/")
test.expectStr(p.base, "baz.txt")
test.expectStr(p.name, "baz")
test.expectStr(p.ext, ".txt")

test.expectStr(path.format(p), "/foo/bar/baz.txt")

test.expectStr(path.sep, "/")
test.expectStr(path.delimiter, ":")
