/// kernel32 — lstr* C-string helpers.

use std.ffi { cstrFromString, freeCstr }
use std.test as test
use windows {
    lstrlenA, lstrcmpA, lstrcmpiA,
}

@extern(C) {
    pub fn run() {
        let hello = cstrFromString("hello")
        test.expect(lstrlenA(hello) as i64, 5)
        freeCstr(hello)

        let empty = cstrFromString("")
        test.expect(lstrlenA(empty) as i64, 0)
        freeCstr(empty)

        let abc = cstrFromString("abc")
        let abc2 = cstrFromString("abc")
        let abd = cstrFromString("abd")
        let ABC = cstrFromString("ABC")

        // lstrcmpA: case-sensitive.
        test.expect(lstrcmpA(abc, abc2) as i64, 0)
        test.expectTrue(lstrcmpA(abc, abd) < 0)
        test.expectTrue(lstrcmpA(abc, ABC) != 0)   // different cases

        // lstrcmpiA: case-insensitive — abc == ABC.
        test.expect(lstrcmpiA(abc, ABC) as i64, 0)

        freeCstr(abc)
        freeCstr(abc2)
        freeCstr(abd)
        freeCstr(ABC)
    }
}

run()
