/// crypt32 — Base64 / hex string encoding round-trips.

use test
use windows {
    CryptBinaryToStringA, CryptStringToBinaryA,
    HeapAlloc, HeapFree, GetProcessHeap,
}

@extern(C) {
    pub fn run() {
        let heap = GetProcessHeap()

        // Encode 3 bytes "abc" to Base64.
        // CRYPT_STRING_BASE64 = 0x00000001.
        // CRYPT_STRING_NOCRLF = 0x40000000 — no trailing newline.
        let flags = 0x40000001 as u32
        let src = HeapAlloc(heap, 0 as u32, 3 as u64)
        // Write 'a','b','c' into src.
        let sb = src as *u8
        // We can't write through *u8 without a helper; instead, use a
        // C string and reinterpret the pointer for the binary form.
        let s = cstrFromString("abc")
        let len: u32 = 0
        // First call sizes the output buffer.
        let ok0 = CryptBinaryToStringA(
            s as *const u8,
            3 as u32,
            flags,
            0 as *char,
            &len
        )
        test.expectTrue(ok0 != 0)
        test.expectTrue(len > 0)

        let outBuf = HeapAlloc(heap, 0 as u32, len as u64)
        let ok1 = CryptBinaryToStringA(
            s as *const u8,
            3 as u32,
            flags,
            outBuf as *char,
            &len
        )
        test.expectTrue(ok1 != 0)
        // "abc" → "YWJj" (well-known Base64).
        test.expectStr(stringFromCstr(outBuf as *const char), "YWJj")

        // Round-trip: decode "YWJj" back to "abc".
        // CRYPT_STRING_BASE64 = 1.
        let inLen: u32 = 4   // "YWJj" length
        let decBuf = HeapAlloc(heap, 0 as u32, 16 as u64)
        let decBufLen: u32 = 16
        let ok2 = CryptStringToBinaryA(
            outBuf as *const char,
            inLen,
            1 as u32,
            decBuf as *u8,
            &decBufLen,
            0 as *u32,
            0 as *u32
        )
        test.expectTrue(ok2 != 0)
        test.expect(decBufLen as i64, 3)

        HeapFree(heap, 0 as u32, outBuf)
        HeapFree(heap, 0 as u32, decBuf)
        HeapFree(heap, 0 as u32, src)
        freeCstr(s)
    }
}

run()
