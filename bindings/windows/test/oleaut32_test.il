/// oleaut32 — BSTR allocation, VARIANT init/clear.

use std.test
use windows {
    SysAllocStringLen, SysFreeString, SysStringLen, SysStringByteLen,
    VARIANT, VariantInit, VariantClear,
}

@extern(C) {
    /// Builds a small UTF-16 string in heap-allocated form via
    /// HeapAlloc, returns a pointer to it. Caller frees with HeapFree.
    /// Avoids needing UTF-16 literals in ilang source.
    pub fn run() {
        // Allocate a BSTR from a UTF-16 string. We construct the UTF-16
        // bytes by hand to avoid needing wide-string literals.
        // "abc" in UTF-16LE: 'a'=0x61, 'b'=0x62, 'c'=0x63.
        let buf: u64 = 0   // 4 u16s on the stack
        // SysAllocStringLen with NULL pointer and length N allocates an
        // uninitialised BSTR of N characters. That's enough to exercise
        // length / free helpers.
        let bstr = SysAllocStringLen(0 as *const u16, 3 as u32)
        test.expectTrue((bstr as i64) != 0)
        test.expect(SysStringLen(bstr) as i64, 3)
        test.expect(SysStringByteLen(bstr) as i64, 6)
        SysFreeString(bstr)

        // SysFreeString of NULL is allowed (no-op).
        SysFreeString(0 as *u16)

        // VARIANT init / clear.
        let v = VARIANT {
            vt: 0,
            wReserved1: 0,
            wReserved2: 0,
            wReserved3: 0,
            data_lo: 0 as u64,
            data_hi: 0 as u64,
        }
        VariantInit(&v)
        // After VariantInit, vt should be 0 (VT_EMPTY).
        test.expect(v.vt as i64, 0)
        let hr = VariantClear(&v)
        test.expect(hr as i64, 0)
    }
}

run()
