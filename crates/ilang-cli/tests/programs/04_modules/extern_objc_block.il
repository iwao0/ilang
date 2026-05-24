// `@extern(ObjC) { ... }` block with `@objc("selector:") fn`
// declarations. The parser desugars each @objc fn into a typed
// `objc_msgSend` alias plus a wrapper that interns the selector
// and forwards. Multiple shapes in one block share the C
// `objc_msgSend` symbol thanks to the L1 alias path. Two blocks
// in the same file coexist via per-block tagged synthetic names.
//
// This fixture is parse + type-check + codegen only — actually
// invoking the wrappers would require libobjc to be loaded at
// run time (macOS-only). We declare the wrappers and reference
// their addresses to keep them alive through DCE, then exit
// cleanly. Real runtime use is exercised by the cocoa bindings.
//
// jit: skip
use std.test

@extern(C) {
    struct objc_object {}
    struct objc_class {}

    // We force `@lib("objc")` to be resolvable at AOT build time
    // by declaring a real libobjc symbol up front. Without this,
    // probe_available_libs wouldn't see "objc" in the lib set
    // and the linker would never pull in libobjc.A.dylib.
    @lib("objc") fn objc_getClass(name: *const char): *objc_class

    // Helper that stays inside the @extern(C) scope so the
    // `cstrFromString` builtin is reachable. Top-level ilang code
    // can't call `cstrFromString` directly (raw pointer escape).
    fn nsobject_class_loaded(): bool {
        let cls = objc_getClass(cstrFromString("NSObject"))
        (cls as i64) != 0
    }
}

@extern(ObjC) {
    // Single-arg receiver pattern (no extra args).
    @objc("alloc")
    fn alloc(cls: *objc_class): *objc_object

    // Receiver + i64 — different sig than `alloc`, sharing the
    // same `objc_msgSend` symbol via the L1 alias path.
    @objc("setTag:")
    fn setTag(obj: *objc_object, tag: i64)

    // Receiver + pointer arg + pointer return.
    @objc("setDelegate:")
    fn setDelegate(obj: *objc_object, delegate: *objc_object): *objc_object
}

@extern(ObjC) {
    // Second block to exercise the per-block tagging: each block
    // synthesises its own `__objc_b<line>c<col>_sel_t` /
    // `__objc_b<line>c<col>_sel_register` / wrapper trio, so two
    // blocks in one file must not collide.
    @objc("hash")
    fn hash(obj: *objc_object): i64
}

// Look up NSObject just to make sure libobjc gets linked at AOT
// build time; we don't dispatch through the wrappers because
// `[NSObject alloc]` etc. needs the AppKit/Foundation framework
// loaded which isn't guaranteed in this test environment.
test.expect(if nsobject_class_loaded() { 1 } else { 0 }, 1)
