// Same `*const u8` parameter that fails at top level is legal
// when the interface lives inside an `@extern(C) {}` block — the
// surrounding scope opts into the C type vocabulary. This shape
// is what `@com interface` bindings (e.g. directx12 IUnknown) rely
// on so they can name `*const GUID` / `**void` in vtable
// signatures.

use std.test as test
@extern(C) {
    @com pub interface IFoo {
        ping(p: *const u8): i32
    }
}

test.expect(1, 1)
