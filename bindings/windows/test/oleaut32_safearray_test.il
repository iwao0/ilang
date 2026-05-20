/// oleaut32 — SAFEARRAY lifecycle (1-D array of i32).

use test
use windows {
    SAFEARRAY, SAFEARRAYBOUND,
    SafeArrayCreate, SafeArrayDestroy,
    SafeArrayGetLBound, SafeArrayGetUBound, SafeArrayGetDim,
    SafeArrayGetElemsize, SafeArrayGetVartype,
    SafeArrayPutElement, SafeArrayGetElement,
    SafeArrayAccessData, SafeArrayUnaccessData,
    VarType,
}

@extern(C) {
    pub fn run() {
        // 1-D array of 5 VT_I4 elements, lower bound 0.
        let bound = SAFEARRAYBOUND { cElements: 5, lLbound: 0 }
        let sa = SafeArrayCreate(VarType.i4 as u16, 1 as u32, bound)
        test.expectTrue((sa as i64) != 0)

        test.expect(SafeArrayGetDim(sa) as i64, 1)
        test.expect(SafeArrayGetElemsize(sa) as i64, 4)   // sizeof i32

        let lb: i32 = -1
        let ub: i32 = -1
        test.expect(SafeArrayGetLBound(sa, 1 as u32, &lb) as i64, 0)
        test.expect(SafeArrayGetUBound(sa, 1 as u32, &ub) as i64, 0)
        test.expect(lb as i64, 0)
        test.expect(ub as i64, 4)

        let vt: u16 = 0
        test.expect(SafeArrayGetVartype(sa, &vt) as i64, 0)
        test.expect(vt as i64, VarType.i4 as i64)

        // Put / get element round-trip.
        let idx: i32 = 2
        let val: i32 = 42
        test.expect(
            SafeArrayPutElement(sa, &idx, &val as *const void) as i64,
            0
        )

        let read: i32 = 0
        test.expect(
            SafeArrayGetElement(sa, &idx, &read as *void) as i64,
            0
        )
        test.expect(read as i64, 42)

        // Access / unaccess data — must be paired.
        let pdata: *void = 0 as *void
        test.expect(SafeArrayAccessData(sa, &pdata) as i64, 0)
        test.expectTrue((pdata as i64) != 0)
        test.expect(SafeArrayUnaccessData(sa) as i64, 0)

        test.expect(SafeArrayDestroy(sa) as i64, 0)
    }
}

run()
