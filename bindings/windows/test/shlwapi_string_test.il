/// shlwapi — string search / convert helpers.

use std.test as test
use shlwapi { StrStrA, StrStrIA, StrCmpNA, StrCmpNIA, StrToIntA }
use std.ffi { cstrFromString, freeCstr }

@extern(C) {
    pub fn run() {
        let hay = cstrFromString("Hello, World!")
        let needle_w = cstrFromString("World")
        let needle_w_lc = cstrFromString("world")
        let needle_miss = cstrFromString("xyz")

        // StrStrA — case-sensitive. "World" found, "world" not.
        let f1 = StrStrA(hay, needle_w)
        test.expectTrue((f1 as i64) != 0)

        let f2 = StrStrA(hay, needle_w_lc)
        test.expect(f2 as i64, 0)

        let f3 = StrStrA(hay, needle_miss)
        test.expect(f3 as i64, 0)

        // StrStrIA — case-insensitive. Both World and world match.
        test.expectTrue((StrStrIA(hay, needle_w) as i64) != 0)
        test.expectTrue((StrStrIA(hay, needle_w_lc) as i64) != 0)
        test.expect(StrStrIA(hay, needle_miss) as i64, 0)

        freeCstr(hay)
        freeCstr(needle_w)
        freeCstr(needle_w_lc)
        freeCstr(needle_miss)

        // StrCmpNA — compare first N chars (case-sensitive).
        let abcd = cstrFromString("abcdef")
        let abcx = cstrFromString("abcxyz")
        test.expect(StrCmpNA(abcd, abcx, 3) as i64, 0)   // first 3 chars equal
        test.expectTrue(StrCmpNA(abcd, abcx, 4) < 0)     // first 4 differ

        // StrCmpNIA — case-insensitive.
        let upper = cstrFromString("ABCXYZ")
        test.expect(StrCmpNIA(abcd, upper, 3) as i64, 0)

        freeCstr(abcd)
        freeCstr(abcx)
        freeCstr(upper)

        // StrToIntA — parses leading decimal integer.
        let n42 = cstrFromString("42")
        test.expect(StrToIntA(n42) as i64, 42)
        freeCstr(n42)

        let neg = cstrFromString("-7")
        test.expect(StrToIntA(neg) as i64, -7)
        freeCstr(neg)

        // Non-digit prefix → 0.
        let bad = cstrFromString("abc")
        test.expect(StrToIntA(bad) as i64, 0)
        freeCstr(bad)
    }
}

run()
