// Built-in `regex` module — wraps Rust's `regex` crate. Smoke-tests
// the public API: test / find / findAll / replace / split, with the
// `i` flag for case-insensitive matching.
use std.regex
use std.test

let r = new regex.Regex("foo+", "i")
test.expectTrue(r.test("Hello FOOO"))
test.expectTrue(!r.test("bar"))

if let some(m) = r.find("yes FOO no") {
    test.expectStr(m, "FOO")
} else {
    console.log("FAIL: expected match")
}
if let some(_) = r.find("nothing") {
    console.log("FAIL: expected none")
}

let all = r.findAll("foo Foo FOO")
test.expect(all.length, 3)
test.expectStr(all[0], "foo")
test.expectStr(all[2], "FOO")

let parts = r.split("a foo b Foo c")
test.expect(parts.length, 3)
test.expectStr(parts[0], "a ")
test.expectStr(parts[2], " c")

let out = r.replace("foo and FOO", "X")
test.expectStr(out, "X and X")
