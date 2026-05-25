// Multi-library fallback: try the primary name first, then any
// additional names. Useful for soname differences (libssl.so.3 vs
// libssl.so.1.1) and OS variants. We give a deliberately-missing
// primary plus a real fallback (libc) and verify the call works.
use std.os as os
use std.test as test
@extern(C) {
    @lib("definitely_not_a_real_lib_xyz999.so", "c") fn abs(x: i32): i32

    // `@lib` declarations only resolve through the extern-C codegen
    // path; wrap the call so ordinary script code can drive it.
    pub fn callAbs(x: i32): i32 { abs(x) }
}

// callAbs(-3) must succeed via the fallback.
test.expect(callAbs(-3) as i64, 3)

// libLoaded reports under the primary name even when only the
// fallback opened. This is the documented contract.
test.expectTrue(os.libLoaded("definitely_not_a_real_lib_xyz999.so"))
