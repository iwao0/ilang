// Multi-library fallback: try the primary name first, then any
// additional names. Useful for soname differences (libssl.so.3 vs
// libssl.so.1.1) and OS variants. We give a deliberately-missing
// primary plus a real fallback (libc) and verify the call works.
use os
use test

@extern(C) {
    @lib("definitely_not_a_real_lib_xyz999.so", "c") fn abs(x: i32): i32
}

// abs(-3) must succeed via the fallback.
test.expect(abs(-3) as i64, 3)

// libLoaded reports under the primary name even when only the
// fallback opened. This is the documented contract.
test.expectTrue(os.libLoaded("definitely_not_a_real_lib_xyz999.so"))
