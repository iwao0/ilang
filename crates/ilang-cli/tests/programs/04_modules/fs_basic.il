// Built-in `fs` module — sync filesystem helpers returning
// `Result<T, fs.FsError>`. Exercises the happy path for read /
// write / append / size / rm and the ENOENT error path for a
// missing file.
use std.fs as fs
use std.test as test
let p = "/tmp/ilang_fs_fixture.txt"

// Clean any leftover from a previous failed run so the test is
// stable across reruns. Ignore the result — file may not exist.
let _ = fs.rm(p)

let w = fs.writeFile(p, "hello")
match w {
    ok(_) {}
    err(e) {
        console.log("writeFile failed:", e.code, e.message)
    }
}

if !fs.exists(p) { console.log("FAIL: expected exists") }
if !fs.isFile(p) { console.log("FAIL: expected isFile") }
if fs.isDir(p) { console.log("FAIL: expected !isDir") }

match fs.readFile(p) {
    ok(s) { test.expectStr(s, "hello") }
    err(e) { console.log("readFile failed:", e.code) }
}

match fs.size(p) {
    ok(n) { test.expect(n, 5) }
    err(e) { console.log("size failed:", e.code) }
}

let _ = fs.appendFile(p, " world")

match fs.readFile(p) {
    ok(s) { test.expectStr(s, "hello world") }
    err(e) { console.log("readFile2 failed:", e.code) }
}

// Error path: missing file surfaces as `ENOENT`.
match fs.readFile("/tmp/ilang_fs_fixture_missing_xyz.txt") {
    ok(_) { console.log("FAIL: unexpected ok") }
    err(e) { test.expectStr(e.code, "ENOENT") }
}

let _ = fs.rm(p)
if fs.exists(p) { console.log("FAIL: expected !exists after rm") }
