// Exercises the Node-style extensions to `fs`: stat / lstat /
// access, copyFile / cp / realpath, chmod / truncate / utimes,
// symlink / readlink / link, and mkdtemp. Each block leaves the
// filesystem clean so the test can be rerun.
use std.fs as fs
use std.test as test
// ─── mkdtemp + cleanup helper ───────────────────────────────────
let root = match fs.mkdtemp("/tmp/ilang_fs_extra_") {
    ok(p) { p }
    err(e) { console.log("mkdtemp failed:", e.code, e.message); "" }
}
test.expectBool(root.length > 0, true)
test.expectBool(fs.isDir(root), true)

// ─── stat ───────────────────────────────────────────────────────
let f1 = root + "/a.txt"
let _ = fs.writeFile(f1, "hello")
match fs.stat(f1) {
    ok(s) {
        test.expect(s.size, 5)
        test.expectBool(s.isFile(), true)
        test.expectBool(s.isDir(), false)
        test.expectBool(s.isSymlink(), false)
        test.expectBool(s.mtimeMs > 0, true)
    }
    err(e) { console.log("stat failed:", e.code) }
}

// ─── access ─────────────────────────────────────────────────────
match fs.access(f1, fs.F_OK) {
    ok(_) {}
    err(e) { console.log("access F_OK failed:", e.code) }
}
match fs.access(f1, fs.R_OK) {
    ok(_) {}
    err(e) { console.log("access R_OK failed:", e.code) }
}
match fs.access(root + "/does_not_exist", fs.F_OK) {
    ok(_) { console.log("FAIL: access on missing path") }
    err(e) { test.expectStr(e.code, "ENOENT") }
}

// ─── copyFile ───────────────────────────────────────────────────
let f2 = root + "/b.txt"
match fs.copyFile(f1, f2, false) {
    ok(_) {}
    err(e) { console.log("copyFile failed:", e.code) }
}
match fs.readFile(f2) {
    ok(s) { test.expectStr(s, "hello") }
    err(e) { console.log("readFile after copy:", e.code) }
}
// overwrite=false should refuse to clobber f2.
match fs.copyFile(f1, f2, false) {
    ok(_) { console.log("FAIL: copyFile no-overwrite clobbered") }
    err(e) { test.expectStr(e.code, "EEXIST") }
}
// overwrite=true succeeds.
match fs.copyFile(f1, f2, true) {
    ok(_) {}
    err(e) { console.log("copyFile overwrite:", e.code) }
}

// ─── realpath ───────────────────────────────────────────────────
match fs.realpath(f1) {
    ok(rp) { test.expectBool(rp.length > 0, true) }
    err(e) { console.log("realpath:", e.code) }
}

// ─── chmod ──────────────────────────────────────────────────────
match fs.chmod(f1, 384 as i32) {   // 0o600
    ok(_) {}
    err(e) { console.log("chmod:", e.code) }
}

// ─── truncate ───────────────────────────────────────────────────
match fs.truncate(f1, 3 as i64) {
    ok(_) {}
    err(e) { console.log("truncate:", e.code) }
}
match fs.size(f1) {
    ok(n) { test.expect(n, 3) }
    err(e) { console.log("size after truncate:", e.code) }
}

// ─── utimes ─────────────────────────────────────────────────────
// Set to 2001-01-01T00:00:00 UTC = 978307200000 ms.
match fs.utimes(f1, 978307200000 as i64, 978307200000 as i64) {
    ok(_) {}
    err(e) { console.log("utimes:", e.code) }
}
match fs.stat(f1) {
    ok(s) { test.expect(s.mtimeMs, 978307200000) }
    err(e) { console.log("stat after utimes:", e.code) }
}

// ─── symlink / readlink / lstat ────────────────────────────────
let sl = root + "/link"
match fs.symlink(f1, sl) {
    ok(_) {}
    err(e) { console.log("symlink:", e.code) }
}
match fs.readlink(sl) {
    ok(t) { test.expectStr(t, f1) }
    err(e) { console.log("readlink:", e.code) }
}
match fs.lstat(sl) {
    ok(s) { test.expectBool(s.isSymlink(), true) }
    err(e) { console.log("lstat:", e.code) }
}
// stat follows the link → reports the target as a regular file.
match fs.stat(sl) {
    ok(s) { test.expectBool(s.isFile(), true) }
    err(e) { console.log("stat through link:", e.code) }
}

// ─── link (hard link) ───────────────────────────────────────────
let hl = root + "/hard"
match fs.link(f1, hl) {
    ok(_) {}
    err(e) { console.log("link:", e.code) }
}
test.expectBool(fs.isFile(hl), true)

// ─── cp -r ──────────────────────────────────────────────────────
let sub = root + "/sub"
let _ = fs.mkdir(sub, false)
let _ = fs.writeFile(sub + "/c.txt", "deep")
let dstDir = root + "/sub-copy"
match fs.cp(sub, dstDir, true) {
    ok(_) {}
    err(e) { console.log("cp -r:", e.code) }
}
match fs.readFile(dstDir + "/c.txt") {
    ok(s) { test.expectStr(s, "deep") }
    err(e) { console.log("readFile copied:", e.code) }
}

// ─── cleanup ────────────────────────────────────────────────────
let _ = fs.rmdir(root, true)
test.expectBool(fs.exists(root), false)
