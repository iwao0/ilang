// `init` is optional. A class with no init can still be `new`'d
// without args; fields just start at zero / "" / etc.
use std.test

class Empty {
    n: i64
    s: string
    flag: bool
}

let e = new Empty()
test.expect(e.n, 0)
test.expectStr(e.s, "")
test.expectBool(e.flag, false)

// Field assignment works even without an init.
e.n = 42
e.s = "hi"
e.flag = true
test.expect(e.n, 42)
test.expectStr(e.s, "hi")
test.expectBool(e.flag, true)
