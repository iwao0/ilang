// `super.method(args)` inside an `@objc class : Parent` body
// dispatches via `objc_msgSendSuper`, so the parent class's
// implementation runs even though our own IMP would otherwise
// shadow the selector. The parser generates a `__super_<method>`
// helper that builds the `struct objc_super` scratch and calls
// the per-shape `objc_msgSendSuper` alias, and rewrites every
// `super.method(args)` AST node into `this.__super_method(args)`.
//
// jit: skip
use test

@extern(ObjC) {
    @objc class NSObject {
        @objc("release") release()
        @objc("hash") hash(): i64
    }

    @objc class MyOverrider : NSObject {
        @objc("alloc") static alloc(): MyOverrider

        // Override `hash` so dispatching `[obj hash]` lands here.
        // The body forwards to NSObject's hash via `super.hash()`
        // (which routes through `objc_msgSendSuper`) and adds 1
        // — so the observable return is (super_hash + 1).
        @objc("hash")
        override hash(): i64 {
            super.hash() + 1
        }
    }
}

MyOverrider.register()
let obj = MyOverrider.alloc()
test.expect(if obj.handle != 0 { 1 } else { 0 }, 1)

// Dispatching `hash` lands in our IMP (super.hash() + 1). Since
// NSObject's hash is non-zero, our result is non-zero too.
let h = obj.hash()
test.expect(if h != 0 { 1 } else { 0 }, 1)

// Sanity: hash is deterministic per object, so two calls match.
let h2 = obj.hash()
test.expect(h, h2)
