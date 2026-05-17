// Metal binding — covers the surface needed to drive
// `SKRenderer.renderWithViewport:commandBuffer:renderPassDescriptor:`
// from ilang code end-to-end (device → command queue → command
// buffer → render-pass descriptor with a colour attachment
// pointing at a freshly-created texture). Not part of the `cocoa`
// umbrella: apps that don't render via Metal shouldn't be forced
// to dlopen `Metal.framework` at startup.
//
// MTLDevice / MTLCommandQueue / MTLCommandBuffer / MTLTexture
// are Apple-side protocols, not classes. The runtime backs each
// with a concrete private class (e.g. `_MTLCommandBuffer`) that
// responds to the protocol's selectors, so `objc_msgSend`
// dispatch works on a handle obtained via
// `MTLCreateSystemDefaultDevice()` or one of the factory
// methods below — we never need `objc_getClass("MTLTexture")`
// to succeed. Don't add `alloc` / `init` on these: protocols
// have no class object to allocate against.
//
// `MTLRenderPassDescriptor`, `MTLRenderPassColorAttachmentDescriptor`,
// `MTLRenderPassColorAttachmentDescriptorArray`, and
// `MTLTextureDescriptor` are concrete classes — the factory
// methods below are the standard constructors.
//
// Higher-tier Metal surface (compile/load shaders via
// `MTLLibrary` / `MTLFunction`, set up `MTLRenderPipelineState`,
// encode draw calls through `MTLRenderCommandEncoder`, raw
// `MTLBuffer` management, compute pipelines) is intentionally
// omitted — it's a lot more surface, and SKRenderer doesn't
// need it. Add per-need.

use foundation { NSObject }

// ─── Enums ────────────────────────────────────────────────────────

/// `MTLPixelFormat` — texture layout / channel format. Only the
/// values likely to feed an SKRenderer pass are listed; consult
/// `MTLPixelFormat.h` for the full set (~200 variants).
pub enum MTLPixelFormat: u64 {
    invalid             = 0
    a8Unorm             = 1
    r8Unorm             = 10
    rg8Unorm            = 30
    rgba8Unorm          = 70
    rgba8Unorm_sRGB     = 71
    bgra8Unorm          = 80
    bgra8Unorm_sRGB     = 81
    rgba16Float         = 115
    rgba32Float         = 125
    depth32Float        = 252
}

/// `MTLLoadAction` — what the GPU does with an attachment's
/// existing contents at the start of a render pass.
pub enum MTLLoadAction: u64 {
    dontCare = 0
    load     = 1
    clear    = 2
}

/// `MTLStoreAction` — what the GPU does with an attachment's
/// contents at the end of a render pass.
pub enum MTLStoreAction: u64 {
    dontCare                   = 0
    store                      = 1
    multisampleResolve         = 2
    storeAndMultisampleResolve = 3
    unknown                    = 4
}

/// `MTLTextureUsage` — bitflag describing how the texture may be
/// used. `RenderTarget` is the bit SKRenderer requires for the
/// colour attachment.
@flags
pub enum MTLTextureUsage: u64 {
    unknown          = 0x0000
    shaderRead       = 0x0001
    shaderWrite      = 0x0002
    renderTarget     = 0x0004
    pixelFormatView  = 0x0010
}

// ─── Structs ──────────────────────────────────────────────────────

@extern(C) {
    /// `MTLClearColor` — RGBA values in `[0, 1]` used when a
    /// colour attachment's `loadAction` is `.clear`. Plain C
    /// struct, 4× f64 = 32 bytes (HFA on arm64, passes via v0–v3
    /// at the ABI boundary).
    pub struct MTLClearColor {
        red: f64
        green: f64
        blue: f64
        alpha: f64
    }
}

// ─── Metal classes / protocols ────────────────────────────────────

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

        @objc("newTextureWithDescriptor:")
        pub newTextureWithDescriptor(d: MTLTextureDescriptor): MTLTexture

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

    /// `id<MTLTexture>` — opaque GPU image. Build via
    /// `device.newTextureWithDescriptor(desc)` from a configured
    /// `MTLTextureDescriptor`, then plug into a render-pass
    /// colour attachment so SKRenderer draws into it. Reading
    /// back to CPU (`getBytes:bytesPerRow:fromRegion:mipmapLevel:`)
    /// isn't bound yet — add per-need.
    @objc pub class MTLTexture : NSObject {
        @objc("width")
        pub width(): u64
        @objc("height")
        pub height(): u64
        @objc("pixelFormat")
        pub pixelFormat(): MTLPixelFormat
    }

    /// `MTLTextureDescriptor` — describes a texture to create.
    /// Use the `texture2DDescriptorWithPixelFormat:width:height:mipmapped:`
    /// factory for the common 2D-target case; the setters below
    /// override the defaults (the factory leaves `usage` as
    /// `.shaderRead` only; an SKRenderer target needs the
    /// `.renderTarget` bit too).
    @objc pub class MTLTextureDescriptor : NSObject {
        @objc("texture2DDescriptorWithPixelFormat:width:height:mipmapped:")
        pub static texture2DDescriptor(
            pixelFormat: MTLPixelFormat,
            width: u64,
            height: u64,
            mipmapped: bool
        ): MTLTextureDescriptor

        @objc("setPixelFormat:")
        pub setPixelFormat(f: MTLPixelFormat)
        @objc("setWidth:")
        pub setWidth(w: u64)
        @objc("setHeight:")
        pub setHeight(h: u64)
        @objc("setUsage:")
        pub setUsage(u: MTLTextureUsage)
        @objc("setStorageMode:")
        pub setStorageMode(m: u64)
    }

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
}

/// Convenience: build an `MTLClearColor` literal. Pure-ilang
/// helper so call sites can keep clear-colour setup to one
/// line: `attachment.setClearColor(metal.clearColor(0, 0, 0, 1))`.
pub fn clearColor(r: f64, g: f64, b: f64, a: f64): MTLClearColor {
    let c = new MTLClearColor()
    c.red = r
    c.green = g
    c.blue = b
    c.alpha = a
    c
}
