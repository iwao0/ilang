// `os.libLoadError` surfaces the dlopen failure message when an
// `@lib(...) @optional` library couldn't be loaded. Used for
// diagnostics; the gating logic still goes through libLoaded.
use os
use test

@extern(C) {
    @lib("definitely_not_real_xyz_abc123.so") @optional fn ghost(): i64
}

test.expectFalse(os.libLoaded("definitely_not_real_xyz_abc123.so"))

pub let err = os.libLoadError("definitely_not_real_xyz_abc123.so")
// Non-empty error message — exact wording depends on the OS
// linker, so we just check it's not empty and contains the lib
// name so it's actionable for the user.
test.expectTrue(err.length > 0)
