/// shlwapi — URL canonicalisation and combine helpers.

use std.test as test
use windows {
use std.ffi { cstrFromString, freeCstr, stringFromCstr }
    HeapAlloc, HeapFree, GetProcessHeap,
}
use shlwapi { UrlIsA, UrlCanonicalizeA, UrlCombineA, UrlCompareA }

@extern(C) {
    pub fn run() {
        let heap = GetProcessHeap()
        let buf = HeapAlloc(heap, 8 as u32, 1024 as u64)

        // UrlIsA: URLIS_URL = 0 — is this a URL at all?
        let url = cstrFromString("https://example.com/foo")
        test.expectTrue(UrlIsA(url, 0 as u32) != 0)
        freeCstr(url)

        let notUrl = cstrFromString("just-a-string")
        test.expect(UrlIsA(notUrl, 0 as u32) as i64, 0)
        freeCstr(notUrl)

        // UrlCanonicalizeA — strips double slashes etc.
        let messy = cstrFromString("https://example.com/a/./b/../c")
        let outLen: u32 = 1024
        let rc = UrlCanonicalizeA(messy, buf as *char, &outLen, 0 as u32)
        test.expect(rc as i64, 0)
        // Canonical form drops the "./" and resolves "..".
        let canon = stringFromCstr(buf as *const char)
        test.expectStr(canon, "https://example.com/a/c")
        freeCstr(messy)

        // UrlCombineA — relative-to-base resolution.
        let base = cstrFromString("https://example.com/dir/page.html")
        let rel = cstrFromString("../other.html")
        outLen = 1024
        let rc2 = UrlCombineA(
            base,
            rel,
            buf as *char,
            &outLen,
            0 as u32
        )
        test.expect(rc2 as i64, 0)
        test.expectStr(
            stringFromCstr(buf as *const char),
            "https://example.com/other.html"
        )
        freeCstr(base)
        freeCstr(rel)

        // UrlCompareA: two equivalent URLs compare 0.
        let a = cstrFromString("HTTPS://EXAMPLE.COM/x")
        let b = cstrFromString("https://example.com/x")
        // fIgnoreSlash=0 → case-sensitive byte compare; URLs above
        // differ in case, so the result is non-zero.
        test.expectTrue(UrlCompareA(a, b, 0 as i32) != 0)
        freeCstr(a)
        freeCstr(b)

        HeapFree(heap, 0 as u32, buf)
    }
}

run()
