// Minimal Metal binding — just enough surface to drive
// `SKRenderer.renderWithViewport:commandBuffer:renderPassDescriptor:`
// from ilang code. Not part of the `cocoa` umbrella: apps that
// don't render via Metal shouldn't be forced to dlopen
// `Metal.framework` at startup.
//
// MTLDevice / MTLCommandQueue / MTLCommandBuffer are Apple-side
// protocols, not classes. The runtime backs each with a concrete
// private class (e.g. `_MTLCommandBuffer`) that responds to the
// protocol's selectors, so `objc_msgSend` dispatch works on a
// handle obtained via `MTLCreateSystemDefaultDevice()` or one of
// the factory methods below — we never need
// `objc_getClass("MTLCommandBuffer")` to succeed. Don't add
// `alloc` / `init` here: protocols have no class object to
// allocate against.
//
// `MTLRenderPassDescriptor` is a concrete class on Apple's side
// — the `renderPassDescriptor` factory below is the standard
// constructor.

use foundation { NSObject }

@extern(ObjC, "/System/Library/Frameworks/Metal.framework/Metal") {
    /// Raw C entrypoint — returns an `id<MTLDevice>` as an `i64`
    /// handle. Users should call the `defaultDevice()` wrapper
    /// below instead so the handle gets boxed into an `MTLDevice`
    /// instance via `__wrap_handle` (matching how the rest of
    /// the @objc surface marshals incoming `id`s).
    @lib pub fn MTLCreateSystemDefaultDevice(): i64

    /// `id<MTLDevice>` — opaque GPU handle. Bridge it to APIs
    /// (e.g. `SKRenderer.rendererWithDevice:`) that take an
    /// arbitrary `NSObject` receiver by passing the instance
    /// through; the @objc dispatch unwraps the underlying id.
    @objc pub class MTLDevice : NSObject {
        /// Wrap the raw `id` returned by
        /// `MTLCreateSystemDefaultDevice()` into a typed
        /// `MTLDevice` instance. The returned device's `handle`
        /// is `0` on machines without Metal support — callers
        /// that want to fail-fast should check `device.handle
        /// != 0` before sending the first message.
        pub static defaultDevice(): MTLDevice {
            MTLDevice.__wrap_handle(MTLCreateSystemDefaultDevice())
        }

        @objc("newCommandQueue")
        pub newCommandQueue(): MTLCommandQueue

        @objc("name")
        pub deviceName(): NSObject
    }

    /// `id<MTLCommandQueue>` — mint command buffers off this.
    /// One queue per device is sufficient for SKRenderer's
    /// use case; reuse across frames.
    @objc pub class MTLCommandQueue : NSObject {
        @objc("commandBuffer")
        pub commandBuffer(): MTLCommandBuffer
    }

    /// `id<MTLCommandBuffer>` — collects encoded GPU work. The
    /// SKRenderer flow is:
    ///   1. `let buf = queue.commandBuffer()`
    ///   2. `renderer.renderWithViewport(rect, buf, desc)`
    ///   3. `buf.commit()`
    ///   4. (optional) `buf.waitUntilCompleted()`
    @objc pub class MTLCommandBuffer : NSObject {
        @objc("commit")
        pub commit()
        @objc("waitUntilScheduled")
        pub waitUntilScheduled()
        @objc("waitUntilCompleted")
        pub waitUntilCompleted()
        // `presentDrawable:` is here for caller-controlled
        // compositing pipelines that target a CAMetalLayer
        // drawable. SKRenderer-direct usage doesn't need it.
        @objc("presentDrawable:")
        pub presentDrawable(drawable: NSObject)
    }

    /// `MTLRenderPassDescriptor` — describes the attachments the
    /// next render pass writes into. For SKRenderer the caller
    /// fills in at least one colour attachment with a texture
    /// the scene draws into; the descriptor's setters aren't
    /// bound here yet because they need
    /// `MTLRenderPassColorAttachmentDescriptor` and friends.
    /// Build the descriptor object, then attach by raw
    /// `objc_msgSend` until those bindings land.
    @objc pub class MTLRenderPassDescriptor : NSObject {
        @objc("renderPassDescriptor")
        pub static renderPassDescriptor(): MTLRenderPassDescriptor
    }
}
