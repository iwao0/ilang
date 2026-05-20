/// MTLRenderPassDescriptor and its colour / depth / stencil
/// attachment family. Built CPU-side, handed to a command buffer
/// via `renderCommandEncoderWithDescriptor:` to start a render pass.

use foundation { NSObject, NSString, NSError, NSRange, withObjcError, objcRetain }
use types.*

@extern(ObjC, "/System/Library/Frameworks/Metal.framework/Metal") {
    /// `MTLRenderPassDescriptor` — describes the attachments the
    /// next render pass writes into. SKRenderer needs at least one
    /// colour attachment with a texture; reach it through
    /// `desc.colorAttachments().objectAtIndexedSubscript(0)` and
    /// configure with the setters on
    /// `MTLRenderPassColorAttachmentDescriptor` below.
    @objc pub class MTLRenderPassDescriptor : NSObject {
        @objc("renderPassDescriptor")
        pub static renderPassDescriptor(): MTLRenderPassDescriptor

        @objc("colorAttachments")
        pub colorAttachments(): MTLRenderPassColorAttachmentDescriptorArray

        /// Depth / stencil attachments. Unlike colour attachments
        /// there's only one slot each (no array), so these come
        /// back as plain attachment descriptors directly. Configure
        /// via the `setTexture:`, `setLoadAction:`, `setStoreAction:`,
        /// and `setClearDepth:` / `setClearStencil:` setters on the
        /// returned descriptor.
        @objc("depthAttachment")
        pub depthAttachment(): MTLRenderPassDepthAttachmentDescriptor
        @objc("setDepthAttachment:")
        pub setDepthAttachment(a: MTLRenderPassDepthAttachmentDescriptor)
        @objc("stencilAttachment")
        pub stencilAttachment(): MTLRenderPassStencilAttachmentDescriptor
        @objc("setStencilAttachment:")
        pub setStencilAttachment(a: MTLRenderPassStencilAttachmentDescriptor)
    }

    /// Subscriptable collection of colour attachments on an
    /// `MTLRenderPassDescriptor`. Apple exposes it as a class on
    /// the ObjC side so the `[arr objectAtIndexedSubscript:i]`
    /// form (i.e. `arr[i]` in Objective-C++) maps to a property
    /// access; ilang code reaches each slot via the
    /// `objectAtIndexedSubscript(i)` selector directly.
    @objc pub class MTLRenderPassColorAttachmentDescriptorArray : NSObject {
        @objc("objectAtIndexedSubscript:")
        pub objectAtIndexedSubscript(idx: u64): MTLRenderPassColorAttachmentDescriptor
        @objc("setObject:atIndexedSubscript:")
        pub setObjectAtIndexedSubscript(
            obj: MTLRenderPassColorAttachmentDescriptor,
            atIndexedSubscript: u64
        )
    }

    /// One slot of `MTLRenderPassDescriptor.colorAttachments`.
    /// SKRenderer at minimum needs `texture` set + `loadAction =
    /// .clear` (or `.load`) + `storeAction = .store`. The
    /// inherited `MTLRenderPassAttachmentDescriptor` selectors
    /// are folded in here directly so callers only need to know
    /// about the one class.
    @objc pub class MTLRenderPassColorAttachmentDescriptor : NSObject {
        @objc("setTexture:")
        pub setTexture(t: MTLTexture)
        @objc("texture")
        pub texture(): MTLTexture
        @objc("setLoadAction:")
        pub setLoadAction(a: MTLLoadAction)
        @objc("setStoreAction:")
        pub setStoreAction(a: MTLStoreAction)
        @objc("setLevel:")
        pub setLevel(l: u64)
        @objc("setSlice:")
        pub setSlice(s: u64)
        @objc("setClearColor:")
        pub setClearColor(c: MTLClearColor)
    }

    /// Depth attachment slot on an `MTLRenderPassDescriptor`.
    /// Inherits load/store/level/slice from
    /// `MTLRenderPassAttachmentDescriptor`; adds `setClearDepth:`
    /// (a `double` in `[0, 1]`, used when `loadAction == .clear`).
    @objc pub class MTLRenderPassDepthAttachmentDescriptor : NSObject {
        @objc("setTexture:")
        pub setTexture(t: MTLTexture)
        @objc("texture")
        pub texture(): MTLTexture
        @objc("setLoadAction:")
        pub setLoadAction(a: MTLLoadAction)
        @objc("setStoreAction:")
        pub setStoreAction(a: MTLStoreAction)
        @objc("setLevel:")
        pub setLevel(l: u64)
        @objc("setSlice:")
        pub setSlice(s: u64)
        @objc("setClearDepth:")
        pub setClearDepth(d: f64)
    }

    /// Stencil attachment slot on an `MTLRenderPassDescriptor`.
    /// Mirrors the depth attachment but with `setClearStencil:`
    /// (a `uint32_t` value).
    @objc pub class MTLRenderPassStencilAttachmentDescriptor : NSObject {
        @objc("setTexture:")
        pub setTexture(t: MTLTexture)
        @objc("texture")
        pub texture(): MTLTexture
        @objc("setLoadAction:")
        pub setLoadAction(a: MTLLoadAction)
        @objc("setStoreAction:")
        pub setStoreAction(a: MTLStoreAction)
        @objc("setLevel:")
        pub setLevel(l: u64)
        @objc("setSlice:")
        pub setSlice(s: u64)
        @objc("setClearStencil:")
        pub setClearStencil(v: u32)
    }
}
