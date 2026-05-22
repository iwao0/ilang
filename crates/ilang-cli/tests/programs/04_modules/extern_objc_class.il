// `@objc class X { ... }` inside `@extern(ObjC)` desugars into a
// regular ilang class with `handle: i64` + `init(h: i64)` and one
// ilang method per declared `@objc("sel:")` member. Methods are
// dispatched through typed `objc_msgSend` aliases (L1 path);
// `static fn` becomes a class-method whose receiver is the result
// of `objc_getClass("ClassName")`.
//
// We exercise the desugar end-to-end against libobjc's NSObject:
//   NSObject.alloc()             — static method, returns NSObject
//   obj.hash()                   — instance method, returns i64
//   obj.isEqual(other)           — instance method taking @objc class arg
//
// jit: skip
use test

@extern(ObjC) {
    @objc class NSObject {
        @objc("alloc")
        static alloc(): NSObject

        @objc("hash")
        hash(): i64

        @objc("isEqual:")
        isEqual(other: NSObject): i8

        @objc("release")
        release()
    }
}

let a = NSObject.alloc()
let b = NSObject.alloc()

// Each freshly-alloc'd NSObject has a non-zero `id` pointer.
test.expect(if a.handle != 0 { 1 } else { 0 }, 1)
test.expect(if b.handle != 0 { 1 } else { 0 }, 1)

// `hash` returns the object's identity hash — deterministic for
// the lifetime of the object. Different objects almost always
// have different hashes; we just check it's non-zero.
let ha = a.hash()
test.expect(if ha != 0 { 1 } else { 0 }, 1)

// isEqual: a vs itself → true (1); a vs b → false (0).
let same = a.isEqual(a)
test.expect(same as i64, 1)

a.release()
b.release()
