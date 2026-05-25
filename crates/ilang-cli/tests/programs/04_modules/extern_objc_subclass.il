// `@objc class Foo : NSObject` registers Foo with the ObjC
// runtime via objc_allocateClassPair + objc_registerClassPair on
// first call to `Foo.register()`. Methods with bodies become
// IMPs attached via class_addMethod, so the ObjC runtime
// dispatches those selectors back into the ilang implementation.
//
// jit: skip
use std.test as test
use std.ffi { cstrFromString }

@extern(ObjC) {
    @objc class NSObject {
        @objc("release") release()
        @objc("hash") hash(): i64
    }

    @objc class MyDispatchTarget : NSObject {
        @objc("alloc") static alloc(): MyDispatchTarget
        @objc("hash") override hash(): i64

        // Body present: this is a subclass override. The parser
        // generates an IMP (`ilang_objc_imp__MyDispatchTarget__answer`)
        // and `register()` attaches it via class_addMethod with
        // type encoding `"q@:"` (returns i64, self + _cmd).
        @objc("answer")
        answer(): i64 {
            12345
        }
    }

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

// Re-running register() is idempotent.
MyDispatchTarget.register()
test.expect(if class_was_registered() { 1 } else { 0 }, 1)

// Allocate an instance and dispatch `hash` through ObjC. With
// no override, the runtime routes to NSObject's hash IMP. The
// result is non-zero (it's the address-derived identity hash).
let obj = MyDispatchTarget.alloc()
test.expect(if obj.handle != 0 { 1 } else { 0 }, 1)
let h = obj.hash()
test.expect(if h != 0 { 1 } else { 0 }, 1)

// Call `answer` — dispatches via msgSend to our IMP, which calls
// __impl_answer, which returns 12345.
let a = obj.answer()
test.expect(a, 12345)
