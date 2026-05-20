// `@objc class Foo : Parent, Iface1, Iface2 { ... }` — extended
// base-list syntax. The first base name (if any) is the
// Objective-C parent class; the rest are ilang interfaces the
// class must conform to. Combines with `@objc interface` declared
// inside the same block to model real Cocoa delegate / data-source
// protocols.
//
// Each method that satisfies an `@objc interface` slot needs the
// matching `@objc("…")` annotation (with the same selector the
// interface declared) so the ObjC runtime invokes the right body.
// Methods not on any interface stay plain @objc.
//
// This fixture is parser / type-checker only — see the runtime
// fixture pair for the actual dispatch verification.
//
// jit: skip
use test

@extern(ObjC) {
    @objc class NSObject {
        @objc("release") fn release()
    }

    @objc pub interface Greeter {
        hello(n: i64)
        @objc("wave:")
        wave?(n: i64)
    }

    @objc class Impl : NSObject, Greeter {
        @objc("alloc") static fn alloc(): Impl
        @objc("init") fn init(): Impl

        @objc("hello:")
        hello(n: i64) {
            // Body would call into ilang from an ObjC dispatch.
        }
    }
}

test.assert(true)
