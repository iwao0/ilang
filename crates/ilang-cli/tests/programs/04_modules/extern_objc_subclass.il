// `@objc class Foo : NSObject` registers Foo with the ObjC
// runtime via objc_allocateClassPair + objc_registerClassPair on
// first call to `Foo.register()`. Until method-override IMP
// support lands, the registered class just inherits its parent's
// methods unchanged — Foo and NSObject share behaviour but Foo
// shows up in libobjc's class registry under its own name.
//
// jit: skip
use test

@extern(ObjC) {
    @objc class NSObject { }

    @objc class MyDispatchTarget : NSObject { }

    // Helper we'll use from the test body to query libobjc for
    // the registered class.
    fn objc_getClass(name: *const char): *MyDispatchTarget

    fn class_was_registered(): bool {
        let cls = objc_getClass(cstrFromString("MyDispatchTarget"))
        (cls as i64) != 0
    }
}

// Before registration, libobjc doesn't know about MyDispatchTarget.
test.expect(if class_was_registered() { 1 } else { 0 }, 0)

MyDispatchTarget.register()

// After registration, the class appears in the runtime's table.
test.expect(if class_was_registered() { 1 } else { 0 }, 1)

// Re-running register() is idempotent — no double-registration.
MyDispatchTarget.register()
test.expect(if class_was_registered() { 1 } else { 0 }, 1)
