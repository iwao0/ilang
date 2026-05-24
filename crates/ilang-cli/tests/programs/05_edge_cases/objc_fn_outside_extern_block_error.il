// `@objc("selector:") fn ...` declarations live inside an
// `@extern(ObjC) { ... }` block and desugar to a typed
// `objc_msgSend` alias plus a wrapper. The wrapper takes
// `*objc_class` / `*objc_object` receivers, which are C-only
// pointer types that can't be constructed (or named) outside an
// `@extern(C) { ... }` block — so trying to dispatch one from
// ordinary code is rejected at type-check time.
//
// expect-error: only allowed inside an @extern(C)
use std.test

@extern(C) {
    struct objc_class {}
    struct objc_object {}
}

@extern(ObjC) {
    @objc("alloc")
    pub fn alloc(cls: *objc_class): *objc_object
}

// Call from outside the @extern(ObjC) block — the `*objc_class`
// cast and the `*objc_object` return both trip the C-only-type
// guard.
let p = alloc(0 as *objc_class)
test.expect(p as i64, 0)
