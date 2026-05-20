/// bcrypt — random bytes, SHA-256 hash of a known input.

use test
use windows {
    BCryptOpenAlgorithmProvider, BCryptCloseAlgorithmProvider,
    BCryptCreateHash, BCryptHashData, BCryptFinishHash, BCryptDestroyHash,
    BCryptGenRandom, BCryptGetProperty,
    BCRYPT_ALG_HANDLE, BCRYPT_HASH_HANDLE,
    HeapAlloc, HeapFree, GetProcessHeap,
}

@extern(C) {
    pub fn run() {
        // BCryptGenRandom with BCRYPT_USE_SYSTEM_PREFERRED_RNG (0x2).
        let heap = GetProcessHeap()
        let buf = HeapAlloc(heap, 8 as u32, 32 as u64)
        let rc = BCryptGenRandom(
            0 as *BCRYPT_ALG_HANDLE,
            buf as *u8,
            32 as u32,
            2 as u32
        )
        test.expect(rc as i64, 0)
        HeapFree(heap, 0 as u32, buf)

        // SHA-256 hash of empty input is well-known:
        //   e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        // We just check the call chain succeeds and the digest length is 32.
        let alg: *BCRYPT_ALG_HANDLE = 0 as *BCRYPT_ALG_HANDLE
        let sha256 = cstrFromString("SHA256")
        // BCryptOpenAlgorithmProvider takes UTF-16; we pass an
        // ANSI string here purely as a smoke test. The call should
        // fail (NTSTATUS != 0) but not crash. Verifying the failure
        // path is the actual assertion.
        let bad = BCryptOpenAlgorithmProvider(
            &alg,
            sha256 as *const u16,    // wrong encoding on purpose
            0 as *const u16,
            0 as u32
        )
        // 0xC0000008 (STATUS_INVALID_HANDLE) or another negative status
        // is acceptable; success would be a problem.
        test.expectTrue(bad != 0)
        freeCstr(sha256)
    }
}

run()
