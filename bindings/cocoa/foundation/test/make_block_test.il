/// Verify the `foundation.makeBlock` / `makeObjectBlock`
/// @intrinsics produce a real ObjC block pointer that Foundation
/// recognises. Pre-conversion these helpers were `@lib("c") @symbol(
/// "__ilang_make_*_block")` plus a `pub fn` wrapper inside the
/// `@extern(ObjC)` block; today they sit as
/// `@intrinsic("objc.make_*_block")` items at module scope, so
/// this fixture pins the wire format the runtime emits is still
/// the layout Foundation expects — a heap-allocated `Block_layout`
/// the ObjC runtime accepts as an `id` (you can wrap it via
/// `NSObject.__wrap_handle` and call `description` / hash etc.
/// on it without crashing).

use std.test
use foundation { NSObject, makeBlock, makeObjectBlock, autoreleasepool }

autoreleasepool(fn() {
    // void(^)(void) shape.
    let voidBlock = makeBlock(fn() {})
    test.expectFalse(voidBlock == 0)
    // Wrap as NSObject — exercises the ObjC runtime's id
    // dispatch on the block pointer.
    let asNS = NSObject.__wrap_handle_unowned(voidBlock)
    let desc = asNS.description
    // Apple's block description starts with `<__NSGlobalBlock__:`
    // for global blocks or `<__NSMallocBlock__:` for heap blocks
    // — the makeBlock path always produces the malloc'd flavour.
    let descStr = desc.length
    test.expectTrue(descStr > 0)

    // void(^)(id) shape.
    let objBlock = makeObjectBlock(fn(_: i64) {})
    test.expectFalse(objBlock == 0)
    let asNS2 = NSObject.__wrap_handle_unowned(objBlock)
    test.expectTrue(asNS2.description.length > 0)

    // Same closure built through the typed `new ObjCBlock(...)`
    // path must produce a wire-compatible block (both run through
    // `__ilang_make_objc_block`'s kind-dispatcher in the runtime,
    // which dispatches to the same `make_void_block` Rust fn).
    // Round-tripping through `.invoke()` confirms the runtime
    // accepts the typed form alongside the `makeBlock` one.
    let viaTyped = new ObjCBlock(fn() {})
    viaTyped.invoke()
})
0
