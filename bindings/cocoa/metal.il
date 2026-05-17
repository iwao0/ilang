// Metal binding — full enough to drive an end-to-end render or
// compute pipeline from ilang code: device → queue → command
// buffer → encoder, plus the resource types each encoder kind
// reaches for. Not part of the `cocoa` umbrella: apps that don't
// render via Metal shouldn't be forced to dlopen
// `Metal.framework` at startup.
//
// MTLDevice / MTLCommandQueue / MTLCommandBuffer /
// MTLTexture / MTLBuffer / MTLLibrary / MTLFunction /
// MTLSamplerState / MTLDepthStencilState /
// MTL{Render,Compute,Blit}CommandEncoder and their pipeline-state
// siblings are Apple-side protocols, not classes. The runtime
// backs each with a concrete private class (e.g.
// `_MTLCommandBuffer`) that responds to the protocol's selectors,
// so `objc_msgSend` dispatch works on a handle obtained via
// `MTLCreateSystemDefaultDevice()` or one of the factory methods
// below — we never need `objc_getClass("MTLTexture")` to
// succeed. Don't add `alloc` / `init` on these: protocols have no
// class object to allocate against.
//
// `MTLRenderPassDescriptor` + its attachment-descriptor family
// (`Color` array + `Depth` / `Stencil` singletons),
// `MTLRenderPipelineDescriptor` + its colour-attachment array,
// `MTLTextureDescriptor`, `MTLVertexDescriptor` + attribute /
// layout arrays, `MTLSamplerDescriptor`, `MTLDepthStencilDescriptor`,
// and `MTLStencilDescriptor` are concrete classes — the factory
// methods below are the standard constructors.
//
// Tier coverage today (each was added in its own commit):
//   * Core pipeline:        device, queue, command buffer, texture,
//                           render-pass, render-pipeline,
//                           library / function, buffer,
//                           vertex descriptor, render encoder
//                           (incl. draw calls)
//   * Tier 3:               compute pipeline + compute encoder
//   * Tier (a):             texture I/O — MTLRegion / MTLOrigin /
//                           MTLSize, replaceRegion / getBytes,
//                           MTLStorageMode
//   * Tier (b):             samplers + depth-stencil state + render
//                           pass depth/stencil attachments
//   * Tier (c):             blit encoder (copy, fill, mipmaps,
//                           managed-storage sync)
//   * Tier (d):             command-buffer completion + schedule
//                           handlers, status / error inspection
//   * Tier (e):             render-encoder rasteriser state
//                           (viewport, scissor, cull, winding,
//                           fill mode, blend colour, RT array
//                           length, debug labels) + compute
//                           threadgroup-memory binding
//   * Tier (g):             MTLHeap (resource pooling) +
//                           MTLFence (intra-queue ordering) +
//                           updateFence/waitForFence on all
//                           three encoder kinds
//   * Tier (h):             MTLEvent + MTLSharedEvent
//                           (cross-queue / CPU↔GPU rendezvous);
//                           encodeSignalEvent: / encodeWaitForEvent:
//                           on the command buffer; CPU-side
//                           signaledValue / setSignaledValue: /
//                           waitUntilSignaledValue:timeoutMS: on
//                           MTLSharedEvent
//
// Surface still intentionally absent (add per-need):
//   * CAMetalLayer / CAMetalDrawable + on-screen presentation
//   * MTLArgumentEncoder + argument buffers
//   * MTLIndirectCommandBuffer (Tier-2 indirect dispatch)
//   * MTLCaptureManager / MTLCaptureScope (Xcode GPU frame capture)
//   * Tessellation / mesh-shading pipeline knobs
//   * Acceleration structures / ray tracing

use foundation { NSObject, NSString, NSError, NSRange, withObjcError, makeObjectBlock }

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
    depth16Unorm        = 250
    depth32Float        = 252
    stencil8            = 253
    depth24Unorm_stencil8 = 255
    depth32Float_stencil8 = 260
    x32_stencil8        = 261
    x24_stencil8        = 262
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

/// `MTLStorageMode` — bare storage-mode enum, used by
/// `MTLTextureDescriptor.setStorageMode(...)`. Sits adjacent to
/// `MTLResourceOptions` (which packs the same modes as bit
/// fields for the buffer factories' `options:` argument).
pub enum MTLStorageMode: u64 {
    shared     = 0
    managed    = 1
    private    = 2
    memoryless = 3
}

/// `MTLResourceOptions` — storage / cache / hazard-tracking mode
/// for a buffer or texture. The shared / managed / private modes
/// are the bits casual callers care about. `storageModeShared` is
/// the default on Apple silicon and what the buffer factories
/// below pass when they want CPU-visible memory.
@flags
pub enum MTLResourceOptions: u64 {
    cpuCacheModeDefault     = 0x0000
    cpuCacheModeWriteCombined = 0x0001
    storageModeShared       = 0x0000
    storageModeManaged      = 0x0010
    storageModePrivate      = 0x0020
    storageModeMemoryless   = 0x0030
    hazardTrackingModeUntracked = 0x0100
}

/// `MTLPrimitiveType` — primitive kind passed to
/// `drawPrimitives:vertexStart:vertexCount:`. Triangle is by far
/// the most common.
pub enum MTLPrimitiveType: u64 {
    point         = 0
    line          = 1
    lineStrip     = 2
    triangle      = 3
    triangleStrip = 4
}

/// `MTLIndexType` — element width of an index buffer fed to
/// `drawIndexedPrimitives:`. Use `uint16` to keep buffers compact
/// when the vertex count fits in 16 bits.
pub enum MTLIndexType: u64 {
    uint16 = 0
    uint32 = 1
}

/// `MTLVertexFormat` — width / channels of one vertex attribute.
/// Listed values cover the common float / int packings; consult
/// `MTLVertexDescriptor.h` for the full set.
pub enum MTLVertexFormat: u64 {
    invalid        = 0
    uchar2         = 1
    uchar3         = 2
    uchar4         = 3
    char2          = 4
    char3          = 5
    char4          = 6
    uchar2Normalized = 7
    uchar3Normalized = 8
    uchar4Normalized = 9
    char2Normalized  = 10
    char3Normalized  = 11
    char4Normalized  = 12
    ushort2        = 13
    ushort3        = 14
    ushort4        = 15
    short2         = 16
    short3         = 17
    short4         = 18
    half2          = 25
    half3          = 26
    half4          = 27
    float          = 28
    float2         = 29
    float3         = 30
    float4         = 31
    int            = 32
    int2           = 33
    int3           = 34
    int4           = 35
    uint           = 36
    uint2          = 37
    uint3          = 38
    uint4          = 39
}

/// `MTLVertexStepFunction` — how the vertex fetch unit advances
/// through a per-vertex buffer. `perVertex` is the standard one;
/// `perInstance` is for instanced rendering.
pub enum MTLVertexStepFunction: u64 {
    constant         = 0
    perVertex        = 1
    perInstance      = 2
}

/// `MTLCompareFunction` — predicate used for depth tests and
/// sampler comparison sampling, and for the stencil-test
/// comparator on each face of a depth-stencil state. `.always`
/// disables the test (every fragment passes).
pub enum MTLCompareFunction: u64 {
    never        = 0
    less         = 1
    equal        = 2
    lessEqual    = 3
    greater      = 4
    notEqual     = 5
    greaterEqual = 6
    always       = 7
}

/// `MTLStencilOperation` — what the GPU does to the stencil
/// buffer value when the stencil test (or the depth test, in the
/// depth-fail slot) fires. `keep` leaves the value untouched.
pub enum MTLStencilOperation: u64 {
    keep           = 0
    zero           = 1
    replace        = 2
    incrementClamp = 3
    decrementClamp = 4
    invert         = 5
    incrementWrap  = 6
    decrementWrap  = 7
}

/// `MTLSamplerMinMagFilter` — interpolation kernel used when the
/// sampled texel grid is denser (min) or sparser (mag) than the
/// destination pixel grid. `linear` does bilinear filtering;
/// `nearest` picks the closest texel.
pub enum MTLSamplerMinMagFilter: u64 {
    nearest = 0
    linear  = 1
}

/// `MTLSamplerMipFilter` — how the sampler picks (and blends) mip
/// levels. `notMipmapped` forces level 0; `linear` does trilinear
/// blending between two adjacent mip levels.
pub enum MTLSamplerMipFilter: u64 {
    notMipmapped = 0
    nearest      = 1
    linear       = 2
}

/// `MTLSamplerAddressMode` — wrap mode for one texture-coordinate
/// axis (s/t/r). `clampToEdge` is the casual default; `repeat` is
/// tiling; `clampToBorderColor` uses `borderColor` for samples
/// outside `[0, 1]`.
pub enum MTLSamplerAddressMode: u64 {
    clampToEdge        = 0
    mirrorClampToEdge  = 1
    repeat             = 2
    mirrorRepeat       = 3
    clampToZero        = 4
    clampToBorderColor = 5
}

/// `MTLSamplerBorderColor` — colour returned for out-of-range
/// samples when address mode is `.clampToBorderColor`.
pub enum MTLSamplerBorderColor: u64 {
    transparentBlack = 0
    opaqueBlack      = 1
    opaqueWhite      = 2
}

/// `MTLHeapType` — placement policy for resources allocated out
/// of an `MTLHeap`. `.automatic` is the casual choice: the driver
/// suballocates from the heap freely and tracks lifetimes for you.
/// `.placement` hands you explicit byte offsets (advanced — for
/// aliasing or transient memory). `.sparse` backs sparse textures.
pub enum MTLHeapType: u64 {
    automatic = 0
    placement = 1
    sparse    = 2
}

/// `MTLRenderStages` — bitmask of pipeline stages a fence /
/// barrier applies to. Used with
/// `MTLRenderCommandEncoder.updateFence:afterStages:` /
/// `waitForFence:beforeStages:`. Pass the OR of the stages the
/// resource is read / written in.
@flags
pub enum MTLRenderStages: u64 {
    vertex   = 0x01
    fragment = 0x02
    tile     = 0x04
    object   = 0x08
    mesh     = 0x10
}

/// `MTLCullMode` — which side of a triangle the rasteriser
/// discards before fragment shading. `.none` disables culling
/// (both sides shade); `.back` is the conventional default.
pub enum MTLCullMode: u64 {
    none  = 0
    front = 1
    back  = 2
}

/// `MTLWinding` — vertex order that constitutes a front-facing
/// triangle. `.counterClockwise` is the OpenGL / Metal default.
pub enum MTLWinding: u64 {
    clockwise        = 0
    counterClockwise = 1
}

/// `MTLTriangleFillMode` — how the rasteriser fills triangle
/// interiors. `.fill` is solid shading; `.lines` is the
/// wireframe-debug mode (no fragment is generated for the
/// interior, only along edges).
pub enum MTLTriangleFillMode: u64 {
    fill  = 0
    lines = 1
}

/// `MTLCommandBufferStatus` — lifecycle position of a command
/// buffer. The values progress notEnqueued → enqueued → committed
/// → scheduled → completed in the happy path; on failure the
/// buffer ends in `error` and `MTLCommandBuffer.error()` returns
/// the diagnostic. Useful to inspect from inside a completion
/// handler.
pub enum MTLCommandBufferStatus: u64 {
    notEnqueued = 0
    enqueued    = 1
    committed   = 2
    scheduled   = 3
    completed   = 4
    error       = 5
}

// ─── Structs ──────────────────────────────────────────────────────

/// `MTLClearColor` — RGBA values in `[0, 1]` used when a colour
/// attachment's `loadAction` is `.clear`. Plain C struct, 4× f64
/// = 32 bytes (HFA on arm64, passes via v0–v3 at the ABI
/// boundary).
pub struct MTLClearColor {
    red: f64
    green: f64
    blue: f64
    alpha: f64
}

/// `MTLSize` — 3D dimensions, in elements (texels for textures,
/// threads for compute dispatch). For 1D dispatches set `height`
/// and `depth` to 1; for 2D set `depth` to 1.
pub struct MTLSize {
    width: u64
    height: u64
    depth: u64
}

/// `MTLOrigin` — 3D coordinate used as the base point of an
/// `MTLRegion`. For 2D textures set `z = 0`; for 1D textures set
/// `y` and `z` to 0.
pub struct MTLOrigin {
    x: u64
    y: u64
    z: u64
}

/// `MTLRegion` — rectangular sub-volume of a texture, used by
/// `MTLTexture.replaceRegion(...)` (upload) and `getBytes(...)`
/// (readback). 48 bytes total = 3× u64 origin + 3× u64 size.
pub struct MTLRegion {
    origin: MTLOrigin
    size: MTLSize
}

/// `MTLViewport` — visible rectangle for the render encoder's
/// rasteriser. `originX` / `originY` and `width` / `height` are
/// in pixels (top-left origin in Metal); `znear` / `zfar` clamp
/// the depth-buffer write to `[0, 1]` typically. 48 bytes total
/// (6× f64) — passes as an HFA on arm64.
pub struct MTLViewport {
    originX: f64
    originY: f64
    width:   f64
    height:  f64
    znear:   f64
    zfar:    f64
}

/// `MTLSizeAndAlign` — the `(size, align)` pair returned by the
/// device's `heapBufferSizeAndAlignWithLength:options:` /
/// `heapTextureSizeAndAlignWithDescriptor:` size-query selectors.
/// Both fields are byte counts. Plain C struct, 16 bytes.
pub struct MTLSizeAndAlign {
    size:  u64
    align: u64
}

/// `MTLScissorRect` — fragment-discard rectangle for the render
/// encoder. Fragments outside `(x, y, x+width, y+height)` are
/// thrown away before reaching the colour attachment. 32 bytes
/// (4× u64).
pub struct MTLScissorRect {
    x:      u64
    y:      u64
    width:  u64
    height: u64
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

        // Shader / pipeline construction. The `error:` parameter
        // is an `NSError **` out-pointer expressed as a raw `i64`
        // slot address — pass `0` to ignore the error, or feed
        // it the per-thread slot via `cocoa.withObjcError(...)`
        // which is what the `OrError` top-level wrappers below
        // use. Returned object handle is 0 on failure.
        @objc("newLibraryWithSource:options:error:")
        pub newLibraryWithSource(
            source: NSString, options: NSObject, error: i64
        ): MTLLibrary
        @objc("newDefaultLibrary")
        pub newDefaultLibrary(): MTLLibrary
        @objc("newRenderPipelineStateWithDescriptor:error:")
        pub newRenderPipelineStateWithDescriptor(
            descriptor: MTLRenderPipelineDescriptor, error: i64
        ): MTLRenderPipelineState
        @objc("newComputePipelineStateWithFunction:error:")
        pub newComputePipelineStateWithFunction(
            function: MTLFunction, error: i64
        ): MTLComputePipelineState

        // Buffer factories. `newBufferWithBytes` copies `length`
        // bytes from `ptr` into a fresh buffer; `newBufferWithLength`
        // allocates uninitialised storage the caller fills via
        // `buffer.contents()`. `options` selects storage mode etc.
        // (`MTLResourceOptions.storageModeShared` is the casual
        // default on Apple silicon).
        @objc("newBufferWithBytes:length:options:")
        pub newBufferWithBytes(
            bytes: i64, length: u64, options: MTLResourceOptions
        ): MTLBuffer
        @objc("newBufferWithLength:options:")
        pub newBufferWithLength(length: u64, options: MTLResourceOptions): MTLBuffer

        // Heap factories + size queries. `newHeapWithDescriptor:`
        // allocates a backing region the caller suballocates from
        // via `heap.newBuffer/Texture(...)`. The two size queries
        // report the (size, align) pair a resource would consume
        // *if* it were placed on a heap with those options — use
        // them to total up the heap size before creating it.
        @objc("newHeapWithDescriptor:")
        pub newHeapWithDescriptor(d: MTLHeapDescriptor): MTLHeap
        @objc("heapBufferSizeAndAlignWithLength:options:")
        pub heapBufferSizeAndAlign(
            length: u64, options: MTLResourceOptions
        ): MTLSizeAndAlign
        @objc("heapTextureSizeAndAlignWithDescriptor:")
        pub heapTextureSizeAndAlign(d: MTLTextureDescriptor): MTLSizeAndAlign

        // Fence factory — a single-queue synchronisation primitive
        // updated by one encoder's `updateFence:` and waited on
        // by a later encoder's `waitForFence:`. Cheap; reuse one
        // per producer/consumer pair per frame.
        @objc("newFence")
        pub newFence(): MTLFence

        // Event factories. `MTLEvent` is a private GPU-only signal
        // (no CPU-visible value); `MTLSharedEvent` exposes a u64
        // counter the CPU can poll, set, and `notifyListener:`
        // wait on — the bridge for CPU↔GPU rendezvous and
        // cross-process / cross-device GPU sync.
        @objc("newEvent")
        pub newEvent(): MTLEvent
        @objc("newSharedEvent")
        pub newSharedEvent(): MTLSharedEvent

        // Sampler / depth-stencil state factories. Both are
        // immutable handles built from a mutable descriptor, the
        // same pattern as `MTLRenderPipelineState`; reuse the
        // resulting state across frames (cheap to bind, more
        // expensive to build).
        @objc("newSamplerStateWithDescriptor:")
        pub newSamplerStateWithDescriptor(d: MTLSamplerDescriptor): MTLSamplerState
        @objc("newDepthStencilStateWithDescriptor:")
        pub newDepthStencilStateWithDescriptor(
            d: MTLDepthStencilDescriptor
        ): MTLDepthStencilState

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
        // Mint a render encoder bound to a render-pass descriptor.
        // The returned encoder records draw calls into this command
        // buffer until `endEncoding` is called.
        @objc("renderCommandEncoderWithDescriptor:")
        pub renderCommandEncoderWithDescriptor(
            descriptor: MTLRenderPassDescriptor
        ): MTLRenderCommandEncoder
        // Mint a compute encoder. The encoder records dispatch
        // calls into this command buffer until `endEncoding`.
        @objc("computeCommandEncoder")
        pub computeCommandEncoder(): MTLComputeCommandEncoder
        // Mint a blit (memory-transfer) encoder. The encoder
        // records buffer ↔ texture copies, mipmap generation, and
        // managed-storage synchronisation into this command
        // buffer until `endEncoding`.
        @objc("blitCommandEncoder")
        pub blitCommandEncoder(): MTLBlitCommandEncoder

        // Lifecycle inspection. `status` advances through the
        // `MTLCommandBufferStatus` enum as the buffer is queued /
        // scheduled / executed; `commandBufferError` returns the
        // `NSError *` that caused a `.error` status (or a 0 handle
        // when none).
        @objc("status")
        pub status(): MTLCommandBufferStatus
        @objc("error")
        pub commandBufferError(): NSError

        // Completion / schedule callbacks. The raw `@objc` forms
        // take a heap-allocated ObjC block pointer (a plain `i64`)
        // built via `foundation.makeObjectBlock(...)`. Prefer the
        // `onCompleted(...)` / `onScheduled(...)` top-level helpers
        // below — they hide the block-creation boilerplate and
        // wrap the incoming raw `id` back into an
        // `MTLCommandBuffer` for the user-supplied closure.
        //
        // Apple documents these handlers as "may be called on
        // a non-main thread"; if the closure has to touch UI
        // state, hop back to the main queue (e.g. via
        // `NSOperationQueue.mainQueue().addOperationWithBlock`).
        @objc("addCompletedHandler:")
        pub addCompletedHandler(block: i64)
        @objc("addScheduledHandler:")
        pub addScheduledHandler(block: i64)

        // GPU-side event signalling / waiting. Both selectors are
        // queued into the command buffer; the GPU executes them
        // in submit order against the event's u64 counter. Use
        // `MTLSharedEvent` if a CPU thread also wants to observe
        // or set the same counter.
        @objc("encodeSignalEvent:value:")
        pub encodeSignalEvent(event: MTLEvent, value: u64)
        @objc("encodeWaitForEvent:value:")
        pub encodeWaitForEvent(event: MTLEvent, value: u64)
    }

    /// `id<MTLTexture>` — opaque GPU image. Build via
    /// `device.newTextureWithDescriptor(desc)` from a configured
    /// `MTLTextureDescriptor`, then plug into a render-pass
    /// colour attachment so SKRenderer draws into it, or move
    /// raw bytes in / out via `replaceRegion(...)` /
    /// `getBytes(...)`. The buffer the caller provides for those
    /// is a raw `i64` pointer — see the
    /// `cocoa.{writeU8, readU8, ...}` helpers (or `malloc` /
    /// `MTLBuffer.contents` if you've got a staging buffer
    /// already in hand).
    @objc pub class MTLTexture : NSObject {
        @objc("width")
        pub width(): u64
        @objc("height")
        pub height(): u64
        @objc("pixelFormat")
        pub pixelFormat(): MTLPixelFormat

        // ─── CPU ↔ GPU byte transfer ────────────────────────────
        //
        // `bytes` is a raw `i64` address (compute via `&local`
        // inside an @extern(C) block, or take it from
        // `mtlbuf.contents()`). `bytesPerRow` is the row stride
        // in bytes — for tight packing pass `region.size.width *
        // bytes_per_pixel` (e.g. 4 for BGRA8Unorm). The storage
        // mode must allow CPU access; private-storage textures
        // need a blit encoder + staging buffer instead.
        @objc("replaceRegion:mipmapLevel:withBytes:bytesPerRow:")
        pub replaceRegion(
            r: MTLRegion,
            mipmapLevel: u64,
            withBytes: i64,
            bytesPerRow: u64
        )
        @objc("replaceRegion:mipmapLevel:slice:withBytes:bytesPerRow:bytesPerImage:")
        pub replaceRegionSlice(
            r: MTLRegion,
            mipmapLevel: u64,
            slice: u64,
            withBytes: i64,
            bytesPerRow: u64,
            bytesPerImage: u64
        )
        @objc("getBytes:bytesPerRow:fromRegion:mipmapLevel:")
        pub getBytes(
            bytes: i64,
            bytesPerRow: u64,
            fromRegion: MTLRegion,
            mipmapLevel: u64
        )
        @objc("getBytes:bytesPerRow:bytesPerImage:fromRegion:mipmapLevel:slice:")
        pub getBytesSlice(
            bytes: i64,
            bytesPerRow: u64,
            bytesPerImage: u64,
            fromRegion: MTLRegion,
            mipmapLevel: u64,
            slice: u64
        )
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
        pub setStorageMode(m: MTLStorageMode)
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

        // Depth / stencil attachments. Unlike colour attachments
        // there's only one slot each (no array), so these come
        // back as plain attachment descriptors directly. Configure
        // via the `setTexture:`, `setLoadAction:`, `setStoreAction:`,
        // and `setClearDepth:` / `setClearStencil:` setters on the
        // returned descriptor.
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

    // ─── Shader library / function ────────────────────────────────

    /// `id<MTLLibrary>` — compiled shader collection. Build via
    /// `device.newLibraryWithSource(src, options, error)` from
    /// Metal Shading Language source, or load the bundle's
    /// default library through `device.newDefaultLibrary()`.
    /// Look up named entry points with `newFunctionWithName`.
    @objc pub class MTLLibrary : NSObject {
        @objc("newFunctionWithName:")
        pub newFunctionWithName(name: NSString): MTLFunction
    }

    /// `id<MTLFunction>` — one compiled shader entry point.
    /// Plug into `MTLRenderPipelineDescriptor.setVertexFunction:`
    /// / `setFragmentFunction:` to wire it into a render
    /// pipeline. Functions are reusable across pipelines.
    @objc pub class MTLFunction : NSObject {
        @objc("name")
        pub functionName(): NSString
    }

    // ─── Buffers ──────────────────────────────────────────────────

    /// `id<MTLBuffer>` — GPU-accessible byte buffer. Built via
    /// `device.newBufferWithBytes` (copy host bytes in) or
    /// `device.newBufferWithLength` (allocate and fill via
    /// `contents()`). `contents()` returns the CPU mapping as a
    /// raw `i64` pointer; `length()` is the buffer size in bytes.
    @objc pub class MTLBuffer : NSObject {
        @objc("contents")
        pub contents(): i64
        @objc("length")
        pub bufferLength(): u64
    }

    // ─── Render pipeline ──────────────────────────────────────────

    /// `MTLRenderPipelineDescriptor` — describes the pipeline the
    /// caller wants compiled. At minimum supply `vertexFunction`,
    /// `fragmentFunction`, and the colour attachment 0 pixel
    /// format (must match the render-pass colour attachment's
    /// texture). `setVertexDescriptor:` is optional — leave it
    /// unset if the shader fetches vertex data through its own
    /// `[[buffer(...)]]` arguments instead of vertex attributes.
    @objc pub class MTLRenderPipelineDescriptor : NSObject {
        @objc("alloc")
        pub static alloc(): MTLRenderPipelineDescriptor
        @objc("init")
        pub init(): MTLRenderPipelineDescriptor

        @objc("setVertexFunction:")
        pub setVertexFunction(f: MTLFunction)
        @objc("setFragmentFunction:")
        pub setFragmentFunction(f: MTLFunction)
        @objc("setVertexDescriptor:")
        pub setVertexDescriptor(v: MTLVertexDescriptor)
        @objc("colorAttachments")
        pub colorAttachments(): MTLRenderPipelineColorAttachmentDescriptorArray

        // Depth / stencil pixel formats for the pipeline. Must
        // match the matching attachment's texture format on the
        // active render pass, otherwise pipeline creation fails.
        // Use `.invalid` (the default) to declare no
        // depth / stencil buffer.
        @objc("setDepthAttachmentPixelFormat:")
        pub setDepthAttachmentPixelFormat(f: MTLPixelFormat)
        @objc("setStencilAttachmentPixelFormat:")
        pub setStencilAttachmentPixelFormat(f: MTLPixelFormat)
    }

    /// Subscriptable collection of colour-attachment slots on an
    /// `MTLRenderPipelineDescriptor`. Apple's ObjC API exposes it
    /// as a real class so the `desc.colorAttachments[0]` form
    /// (`[arr objectAtIndexedSubscript:0]`) can act as a property
    /// reference.
    @objc pub class MTLRenderPipelineColorAttachmentDescriptorArray : NSObject {
        @objc("objectAtIndexedSubscript:")
        pub objectAtIndexedSubscript(idx: u64): MTLRenderPipelineColorAttachmentDescriptor
    }

    /// One slot in `MTLRenderPipelineDescriptor.colorAttachments`.
    /// At minimum set `pixelFormat` to match the render-pass
    /// colour attachment's texture; blending setters are listed
    /// here for the common cases (enable + simple source-over
    /// alpha blending). Mask / write setters / clamp setters
    /// aren't bound yet — add per-need.
    @objc pub class MTLRenderPipelineColorAttachmentDescriptor : NSObject {
        @objc("setPixelFormat:")
        pub setPixelFormat(f: MTLPixelFormat)
        @objc("setBlendingEnabled:")
        pub setBlendingEnabled(flag: bool)
        @objc("setRgbBlendOperation:")
        pub setRgbBlendOperation(op: u64)
        @objc("setAlphaBlendOperation:")
        pub setAlphaBlendOperation(op: u64)
        @objc("setSourceRGBBlendFactor:")
        pub setSourceRGBBlendFactor(f: u64)
        @objc("setSourceAlphaBlendFactor:")
        pub setSourceAlphaBlendFactor(f: u64)
        @objc("setDestinationRGBBlendFactor:")
        pub setDestinationRGBBlendFactor(f: u64)
        @objc("setDestinationAlphaBlendFactor:")
        pub setDestinationAlphaBlendFactor(f: u64)
    }

    /// `id<MTLRenderPipelineState>` — compiled pipeline ready to
    /// be bound on a render encoder. Built via
    /// `device.newRenderPipelineStateWithDescriptor(desc, error)`;
    /// expensive to create (~ms), so cache across frames.
    @objc pub class MTLRenderPipelineState : NSObject {}

    // ─── Vertex descriptor ────────────────────────────────────────

    /// `MTLVertexDescriptor` — declares how the vertex fetch unit
    /// reads attribute data out of bound `MTLBuffer`s. Each
    /// element of `attributes` describes one attribute (format
    /// + offset + which buffer slot it lives in); each element
    /// of `layouts` describes a buffer slot (stride + step
    /// function + step rate). Most simple samples can skip this
    /// and use `[[buffer(...)]]` indexing inside the shader.
    @objc pub class MTLVertexDescriptor : NSObject {
        @objc("vertexDescriptor")
        pub static vertexDescriptor(): MTLVertexDescriptor

        @objc("attributes")
        pub attributes(): MTLVertexAttributeDescriptorArray
        @objc("layouts")
        pub layouts(): MTLVertexBufferLayoutDescriptorArray
    }

    /// Subscriptable collection of `MTLVertexAttributeDescriptor`s.
    @objc pub class MTLVertexAttributeDescriptorArray : NSObject {
        @objc("objectAtIndexedSubscript:")
        pub objectAtIndexedSubscript(idx: u64): MTLVertexAttributeDescriptor
    }

    /// One attribute slot. `bufferIndex` selects which buffer the
    /// fetch unit reads from; `offset` is the byte offset into a
    /// single element of that buffer; `format` describes the
    /// width / channel count.
    @objc pub class MTLVertexAttributeDescriptor : NSObject {
        @objc("setFormat:")
        pub setFormat(f: MTLVertexFormat)
        @objc("setOffset:")
        pub setOffset(o: u64)
        @objc("setBufferIndex:")
        pub setBufferIndex(i: u64)
    }

    /// Subscriptable collection of `MTLVertexBufferLayoutDescriptor`s.
    @objc pub class MTLVertexBufferLayoutDescriptorArray : NSObject {
        @objc("objectAtIndexedSubscript:")
        pub objectAtIndexedSubscript(idx: u64): MTLVertexBufferLayoutDescriptor
    }

    /// One buffer-layout slot. `stride` is the byte distance
    /// between vertex elements; `stepFunction` is usually
    /// `.perVertex`; `stepRate` is 1 except for non-standard
    /// instancing.
    @objc pub class MTLVertexBufferLayoutDescriptor : NSObject {
        @objc("setStride:")
        pub setStride(s: u64)
        @objc("setStepFunction:")
        pub setStepFunction(f: MTLVertexStepFunction)
        @objc("setStepRate:")
        pub setStepRate(r: u64)
    }

    // ─── Render encoder ───────────────────────────────────────────

    /// `id<MTLRenderCommandEncoder>` — records draw calls into the
    /// parent `MTLCommandBuffer`. Bind a pipeline state, set
    /// buffers / textures / sampler states, issue
    /// `drawPrimitives` (or `drawIndexedPrimitives`), then call
    /// `endEncoding` before committing the command buffer.
    @objc pub class MTLRenderCommandEncoder : NSObject {
        @objc("setRenderPipelineState:")
        pub setRenderPipelineState(s: MTLRenderPipelineState)

        // Rasteriser state. `setViewport:` sets the visible
        // rectangle (and depth-clip range) the fragments are
        // written into; `setScissorRect:` discards fragments
        // outside the supplied rectangle. Cull / winding / fill
        // configure the triangle pipeline:
        //   * cull mode    — `.back` is the casual default
        //   * winding      — `.counterClockwise` (front-facing) is
        //                    the Metal default
        //   * fill mode    — `.fill` is solid; `.lines` is wireframe
        // `setRenderTargetArrayLength:` toggles between rendering
        // into a single layer (0) and multi-layered rendering
        // (matching the texture's `arrayLength` or `depth`).
        // `setBlendColorRed:green:blue:alpha:` feeds the constant
        // blend factor used by `.sourceConstantColor` /
        // `.destinationConstantColor` blend factors.
        @objc("setViewport:")
        pub setViewport(v: MTLViewport)
        @objc("setScissorRect:")
        pub setScissorRect(r: MTLScissorRect)
        @objc("setCullMode:")
        pub setCullMode(m: MTLCullMode)
        @objc("setFrontFacingWinding:")
        pub setFrontFacingWinding(w: MTLWinding)
        @objc("setTriangleFillMode:")
        pub setTriangleFillMode(m: MTLTriangleFillMode)
        @objc("setRenderTargetArrayLength:")
        pub setRenderTargetArrayLength(n: u64)
        @objc("setBlendColorRed:green:blue:alpha:")
        pub setBlendColor(r: f32, green: f32, blue: f32, alpha: f32)

        // Intra-queue fence ordering. `updateFence:afterStages:`
        // signals the fence after the named pipeline stages
        // complete; `waitForFence:beforeStages:` blocks the named
        // stages from starting until the fence is signalled. The
        // stage bitmask is `MTLRenderStages` — typically
        // `.vertex | .fragment` for a full-pipeline barrier.
        @objc("updateFence:afterStages:")
        pub updateFence(fence: MTLFence, afterStages: MTLRenderStages)
        @objc("waitForFence:beforeStages:")
        pub waitForFence(fence: MTLFence, beforeStages: MTLRenderStages)

        // Debug labels — visible in Xcode's GPU frame capture. The
        // push/pop pair brackets a span of encoder calls; `setLabel`
        // names the encoder itself.
        @objc("setLabel:")
        pub setLabel(s: NSString)
        @objc("pushDebugGroup:")
        pub pushDebugGroup(s: NSString)
        @objc("popDebugGroup")
        pub popDebugGroup()

        @objc("setVertexBuffer:offset:atIndex:")
        pub setVertexBuffer(b: MTLBuffer, offset: u64, atIndex: u64)
        @objc("setVertexBytes:length:atIndex:")
        pub setVertexBytes(bytes: i64, length: u64, atIndex: u64)
        @objc("setVertexTexture:atIndex:")
        pub setVertexTexture(t: MTLTexture, atIndex: u64)

        @objc("setFragmentBuffer:offset:atIndex:")
        pub setFragmentBuffer(b: MTLBuffer, offset: u64, atIndex: u64)
        @objc("setFragmentBytes:length:atIndex:")
        pub setFragmentBytes(bytes: i64, length: u64, atIndex: u64)
        @objc("setFragmentTexture:atIndex:")
        pub setFragmentTexture(t: MTLTexture, atIndex: u64)

        // Sampler-state bindings — one per shader stage, indexed by
        // the `[[sampler(n)]]` slot in the shader.
        @objc("setVertexSamplerState:atIndex:")
        pub setVertexSamplerState(s: MTLSamplerState, atIndex: u64)
        @objc("setFragmentSamplerState:atIndex:")
        pub setFragmentSamplerState(s: MTLSamplerState, atIndex: u64)

        // Depth / stencil pipeline. `setDepthStencilState:` binds
        // a compiled depth-stencil state; `setStencilReferenceValue:`
        // sets the reference compared against the buffer; the
        // depth-bias triple feeds the rasteriser's polygon-offset.
        @objc("setDepthStencilState:")
        pub setDepthStencilState(s: MTLDepthStencilState)
        @objc("setStencilReferenceValue:")
        pub setStencilReferenceValue(ref: u32)
        @objc("setDepthBias:slopeScale:clamp:")
        pub setDepthBias(depthBias: f32, slopeScale: f32, clamp: f32)

        @objc("drawPrimitives:vertexStart:vertexCount:")
        pub drawPrimitives(
            primitiveType: MTLPrimitiveType, vertexStart: u64, vertexCount: u64
        )
        @objc("drawPrimitives:vertexStart:vertexCount:instanceCount:")
        pub drawPrimitivesInstanced(
            primitiveType: MTLPrimitiveType,
            vertexStart: u64,
            vertexCount: u64,
            instanceCount: u64
        )
        @objc("drawIndexedPrimitives:indexCount:indexType:indexBuffer:indexBufferOffset:")
        pub drawIndexedPrimitives(
            primitiveType: MTLPrimitiveType,
            indexCount: u64,
            indexType: MTLIndexType,
            indexBuffer: MTLBuffer,
            indexBufferOffset: u64
        )

        @objc("endEncoding")
        pub endEncoding()
    }

    // ─── Compute pipeline ─────────────────────────────────────────

    /// `id<MTLComputePipelineState>` — compiled compute kernel
    /// ready to bind on a compute encoder. Built via
    /// `device.newComputePipelineStateWithFunction(kernelFn, err)`
    /// from an `MTLFunction` whose source was declared
    /// `kernel ...` in the shader. Three read-only properties
    /// expose the limits the runtime computed for the kernel:
    /// `threadExecutionWidth` (preferred multiple for
    /// threadgroup width), `maxTotalThreadsPerThreadgroup`
    /// (upper bound), and `staticThreadgroupMemoryLength`.
    @objc pub class MTLComputePipelineState : NSObject {
        @objc("threadExecutionWidth")
        pub threadExecutionWidth(): u64
        @objc("maxTotalThreadsPerThreadgroup")
        pub maxTotalThreadsPerThreadgroup(): u64
        @objc("staticThreadgroupMemoryLength")
        pub staticThreadgroupMemoryLength(): u64
    }

    /// `id<MTLComputeCommandEncoder>` — records compute dispatch
    /// calls into the parent command buffer. Bind a pipeline
    /// state, attach buffers / textures, then dispatch with one
    /// of:
    ///   * `dispatchThreads:threadsPerThreadgroup:` — Apple silicon
    ///     (and recent discrete GPUs) only; thread count needn't
    ///     be a multiple of the threadgroup size.
    ///   * `dispatchThreadgroups:threadsPerThreadgroup:` — works
    ///     on every Metal-capable device; total threads =
    ///     threadgroups × threadsPerThreadgroup.
    // ─── Heap (resource pooling) ──────────────────────────────────

    /// `MTLHeapDescriptor` — pre-creation knobs for an `MTLHeap`.
    /// Set at minimum `size` and (usually) `storageMode`; leave
    /// `type` at the `.automatic` default unless you have a
    /// reason to manage offsets yourself. `setResourceOptions:`
    /// composes storage / cache / hazard-tracking bits in one
    /// call instead of three.
    @objc pub class MTLHeapDescriptor : NSObject {
        @objc("alloc")
        pub static alloc(): MTLHeapDescriptor
        @objc("init")
        pub init(): MTLHeapDescriptor

        @objc("setSize:")
        pub setSize(s: u64)
        @objc("setStorageMode:")
        pub setStorageMode(m: MTLStorageMode)
        @objc("setHazardTrackingMode:")
        pub setHazardTrackingMode(m: u64)
        @objc("setResourceOptions:")
        pub setResourceOptions(o: MTLResourceOptions)
        @objc("setType:")
        pub setType(t: MTLHeapType)
    }

    /// `id<MTLHeap>` — pooled GPU memory. Build via
    /// `device.newHeapWithDescriptor(desc)`, then carve out
    /// buffers / textures with `heap.newBuffer/Texture(...)`. The
    /// pool's lifetime owns every suballocation; releasing the
    /// heap frees everything it holds.
    ///
    /// `usedSize` is the running total of bytes allocated out of
    /// the heap; `currentAllocatedSize` is what the driver has
    /// physically reserved (≥ usedSize, may grow on demand for
    /// `.automatic` heaps). `maxAvailableSizeWithAlignment:`
    /// queries the largest contiguous block of free space.
    @objc pub class MTLHeap : NSObject {
        @objc("newBufferWithLength:options:")
        pub newBufferWithLength(
            length: u64, options: MTLResourceOptions
        ): MTLBuffer
        @objc("newTextureWithDescriptor:")
        pub newTextureWithDescriptor(d: MTLTextureDescriptor): MTLTexture

        @objc("size")
        pub size(): u64
        @objc("usedSize")
        pub usedSize(): u64
        @objc("currentAllocatedSize")
        pub currentAllocatedSize(): u64
        @objc("maxAvailableSizeWithAlignment:")
        pub maxAvailableSizeWithAlignment(align: u64): u64

        @objc("setLabel:")
        pub setLabel(s: NSString)
        @objc("label")
        pub label(): NSString
    }

    // ─── Fence (intra-queue synchronisation) ──────────────────────

    /// `id<MTLFence>` — single-queue synchronisation token. One
    /// encoder calls `updateFence:` to "set" it after its work;
    /// a later encoder on the same queue calls `waitForFence:`
    /// to gate its work until the fence is set. Use this when an
    /// untracked-storage heap or a `useResource:` declared
    /// resource needs explicit ordering — automatic hazard
    /// tracking handles tracked resources for you. For
    /// cross-queue or CPU-side waits, reach for `MTLEvent` /
    /// `MTLSharedEvent` instead.
    @objc pub class MTLFence : NSObject {
        @objc("setLabel:")
        pub setLabel(s: NSString)
        @objc("label")
        pub label(): NSString
    }

    // ─── Events (CPU↔GPU / cross-queue synchronisation) ─────────

    /// `id<MTLEvent>` — opaque, GPU-only synchronisation token
    /// whose value is a 64-bit counter. Use
    /// `commandBuffer.encodeSignalEvent(event, value)` to publish
    /// a new value and `encodeWaitForEvent(event, value)` to
    /// gate later GPU work on the counter reaching that value.
    /// No CPU side — for that, use `MTLSharedEvent`.
    @objc pub class MTLEvent : NSObject {
        @objc("setLabel:")
        pub setLabel(s: NSString)
        @objc("label")
        pub label(): NSString
    }

    /// `id<MTLSharedEvent>` — same counter semantics as
    /// `MTLEvent`, but the CPU can also read (`signaledValue`),
    /// write (`setSignaledValue:`), and wait
    /// (`waitUntilSignaledValue:timeoutMS:`) on it, and can
    /// register a block-based listener for value transitions
    /// (`notifyListener:atValue:block:`).
    ///
    /// Use this to (a) tell the GPU to wait for the CPU
    /// (`encodeWaitForEvent(ev, n)` on the GPU side, then bump
    /// the value from the CPU), or (b) the reverse (GPU signals
    /// via `encodeSignalEvent`, CPU waits via the
    /// `wait…timeoutMS:` selector below). Inherits the GPU-side
    /// signal/wait API by responding to the `MTLEvent`
    /// selectors transparently.
    @objc pub class MTLSharedEvent : MTLEvent {
        @objc("signaledValue")
        pub signaledValue(): u64
        @objc("setSignaledValue:")
        pub setSignaledValue(v: u64)
        @objc("waitUntilSignaledValue:timeoutMS:")
        pub waitUntilSignaledValue(value: u64, timeoutMS: u64): bool
        // The notify listener selector takes an
        // `MTLSharedEventListener` + value + `void(^)(id<MTLSharedEvent>, u64)`
        // block; bind that when there's a real CPU-side handler
        // need — for the casual single-poll case
        // `waitUntilSignaledValue:` is enough.

        // setLabel / label are inherited from MTLEvent.
    }

    // ─── Sampler state ────────────────────────────────────────────

    /// `MTLSamplerDescriptor` — describes how a texture is
    /// filtered / addressed when sampled in the shader. Build via
    /// `MTLSamplerDescriptor.alloc().init()` (no class-level
    /// factory exists), tune the filter / address / clamp / lod
    /// values, then bake into a state with
    /// `device.newSamplerStateWithDescriptor(desc)`. The descriptor
    /// is mutable and one-shot — the resulting `MTLSamplerState`
    /// captures the values; you can free the descriptor afterwards.
    @objc pub class MTLSamplerDescriptor : NSObject {
        @objc("alloc")
        pub static alloc(): MTLSamplerDescriptor
        @objc("init")
        pub init(): MTLSamplerDescriptor

        @objc("setMinFilter:")
        pub setMinFilter(f: MTLSamplerMinMagFilter)
        @objc("setMagFilter:")
        pub setMagFilter(f: MTLSamplerMinMagFilter)
        @objc("setMipFilter:")
        pub setMipFilter(f: MTLSamplerMipFilter)
        @objc("setSAddressMode:")
        pub setSAddressMode(m: MTLSamplerAddressMode)
        @objc("setTAddressMode:")
        pub setTAddressMode(m: MTLSamplerAddressMode)
        @objc("setRAddressMode:")
        pub setRAddressMode(m: MTLSamplerAddressMode)
        @objc("setBorderColor:")
        pub setBorderColor(c: MTLSamplerBorderColor)
        @objc("setNormalizedCoordinates:")
        pub setNormalizedCoordinates(flag: bool)
        @objc("setMaxAnisotropy:")
        pub setMaxAnisotropy(n: u64)
        @objc("setLodMinClamp:")
        pub setLodMinClamp(v: f32)
        @objc("setLodMaxClamp:")
        pub setLodMaxClamp(v: f32)
        @objc("setCompareFunction:")
        pub setCompareFunction(f: MTLCompareFunction)
        @objc("setLabel:")
        pub setLabel(s: NSString)
    }

    /// `id<MTLSamplerState>` — compiled sampler ready to bind on
    /// an encoder via the various `setVertex/FragmentSamplerState:`
    /// / `setSamplerState:` selectors. Cheap to keep across
    /// frames; build once per material.
    @objc pub class MTLSamplerState : NSObject {}

    // ─── Depth-stencil state ──────────────────────────────────────

    /// `MTLStencilDescriptor` — one face's stencil-test
    /// configuration. Embedded inside an `MTLDepthStencilDescriptor`
    /// twice (front face / back face); pass the same descriptor
    /// to both setters when both faces should use identical
    /// rules. `setStencilCompareFunction:` selects the predicate;
    /// the three operation setters pick what to write to the
    /// stencil buffer on stencil-fail / depth-fail / pass.
    @objc pub class MTLStencilDescriptor : NSObject {
        @objc("alloc")
        pub static alloc(): MTLStencilDescriptor
        @objc("init")
        pub init(): MTLStencilDescriptor

        @objc("setStencilCompareFunction:")
        pub setStencilCompareFunction(f: MTLCompareFunction)
        @objc("setStencilFailureOperation:")
        pub setStencilFailureOperation(op: MTLStencilOperation)
        @objc("setDepthFailureOperation:")
        pub setDepthFailureOperation(op: MTLStencilOperation)
        @objc("setDepthStencilPassOperation:")
        pub setDepthStencilPassOperation(op: MTLStencilOperation)
        @objc("setReadMask:")
        pub setReadMask(m: u32)
        @objc("setWriteMask:")
        pub setWriteMask(m: u32)
    }

    /// `MTLDepthStencilDescriptor` — combined depth + stencil
    /// configuration. Build via
    /// `MTLDepthStencilDescriptor.alloc().init()`, then bake with
    /// `device.newDepthStencilStateWithDescriptor(desc)`. The
    /// depth side has just a compare function (`.always` disables
    /// the test) + write flag; the stencil side wires two
    /// `MTLStencilDescriptor`s in.
    @objc pub class MTLDepthStencilDescriptor : NSObject {
        @objc("alloc")
        pub static alloc(): MTLDepthStencilDescriptor
        @objc("init")
        pub init(): MTLDepthStencilDescriptor

        @objc("setDepthCompareFunction:")
        pub setDepthCompareFunction(f: MTLCompareFunction)
        @objc("setDepthWriteEnabled:")
        pub setDepthWriteEnabled(flag: bool)
        @objc("setFrontFaceStencil:")
        pub setFrontFaceStencil(s: MTLStencilDescriptor)
        @objc("setBackFaceStencil:")
        pub setBackFaceStencil(s: MTLStencilDescriptor)
        @objc("setLabel:")
        pub setLabel(s: NSString)
    }

    /// `id<MTLDepthStencilState>` — compiled depth/stencil state
    /// bound onto a render encoder via `setDepthStencilState:`.
    /// Build once per render mode and reuse.
    @objc pub class MTLDepthStencilState : NSObject {}

    // ─── Render-pass depth / stencil attachments ─────────────────

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

    @objc pub class MTLComputeCommandEncoder : NSObject {
        @objc("setComputePipelineState:")
        pub setComputePipelineState(s: MTLComputePipelineState)

        @objc("setBuffer:offset:atIndex:")
        pub setBuffer(b: MTLBuffer, offset: u64, atIndex: u64)
        @objc("setBytes:length:atIndex:")
        pub setBytes(bytes: i64, length: u64, atIndex: u64)
        @objc("setTexture:atIndex:")
        pub setTexture(t: MTLTexture, atIndex: u64)
        @objc("setSamplerState:atIndex:")
        pub setSamplerState(s: MTLSamplerState, atIndex: u64)

        // Reserve threadgroup-local memory for a specific binding
        // slot (`threadgroup` qualified storage in the kernel).
        // `length` is in bytes; the kernel must declare a matching
        // `[[threadgroup(n)]]` parameter at `atIndex`.
        @objc("setThreadgroupMemoryLength:atIndex:")
        pub setThreadgroupMemoryLength(length: u64, atIndex: u64)

        @objc("setLabel:")
        pub setLabel(s: NSString)
        @objc("pushDebugGroup:")
        pub pushDebugGroup(s: NSString)
        @objc("popDebugGroup")
        pub popDebugGroup()

        @objc("dispatchThreads:threadsPerThreadgroup:")
        pub dispatchThreads(
            threads: MTLSize, threadsPerThreadgroup: MTLSize
        )
        @objc("dispatchThreadgroups:threadsPerThreadgroup:")
        pub dispatchThreadgroups(
            threadgroups: MTLSize, threadsPerThreadgroup: MTLSize
        )

        // Intra-queue fences. Compute encoders don't have a
        // stage mask — the whole dispatch is the "stage".
        @objc("updateFence:")
        pub updateFence(fence: MTLFence)
        @objc("waitForFence:")
        pub waitForFence(fence: MTLFence)

        @objc("endEncoding")
        pub endEncoding()
    }

    // ─── Blit encoder ─────────────────────────────────────────────

    /// `id<MTLBlitCommandEncoder>` — records memory-transfer work
    /// (buffer ↔ buffer, buffer ↔ texture, texture ↔ texture) and
    /// resource maintenance (mipmap generation, managed-storage
    /// CPU/GPU synchronisation) into the parent command buffer.
    /// Mint via `cmdBuf.blitCommandEncoder()`, queue copies, then
    /// `endEncoding` before committing.
    ///
    /// The copy selectors take raw `MTLOrigin` / `MTLSize` structs
    /// (build with `metal.origin(...)` / `metal.size(...)`), so
    /// `bytesPerRow` and `bytesPerImage` are caller-computed —
    /// `width * bytesPerTexel` for tight 2D packing,
    /// `bytesPerRow * height` for tight 3D packing.
    @objc pub class MTLBlitCommandEncoder : NSObject {
        // Buffer ↔ buffer byte copy.
        @objc("copyFromBuffer:sourceOffset:toBuffer:destinationOffset:size:")
        pub copyFromBufferToBuffer(
            src: MTLBuffer,
            sourceOffset: u64,
            toBuffer: MTLBuffer,
            destinationOffset: u64,
            length: u64
        )

        // Buffer → texture upload. `sourceBytesPerImage` should be
        // 0 for 2D destinations (no slice stride).
        @objc("copyFromBuffer:sourceOffset:sourceBytesPerRow:sourceBytesPerImage:sourceSize:toTexture:destinationSlice:destinationLevel:destinationOrigin:")
        pub copyFromBufferToTexture(
            src: MTLBuffer,
            sourceOffset: u64,
            sourceBytesPerRow: u64,
            sourceBytesPerImage: u64,
            sourceSize: MTLSize,
            toTexture: MTLTexture,
            destinationSlice: u64,
            destinationLevel: u64,
            destinationOrigin: MTLOrigin
        )

        // Texture → buffer readback. Mirrors the upload variant.
        @objc("copyFromTexture:sourceSlice:sourceLevel:sourceOrigin:sourceSize:toBuffer:destinationOffset:destinationBytesPerRow:destinationBytesPerImage:")
        pub copyFromTextureToBuffer(
            src: MTLTexture,
            sourceSlice: u64,
            sourceLevel: u64,
            sourceOrigin: MTLOrigin,
            sourceSize: MTLSize,
            toBuffer: MTLBuffer,
            destinationOffset: u64,
            destinationBytesPerRow: u64,
            destinationBytesPerImage: u64
        )

        // Texture → texture copy of a single region.
        @objc("copyFromTexture:sourceSlice:sourceLevel:sourceOrigin:sourceSize:toTexture:destinationSlice:destinationLevel:destinationOrigin:")
        pub copyFromTextureToTexture(
            src: MTLTexture,
            sourceSlice: u64,
            sourceLevel: u64,
            sourceOrigin: MTLOrigin,
            sourceSize: MTLSize,
            toTexture: MTLTexture,
            destinationSlice: u64,
            destinationLevel: u64,
            destinationOrigin: MTLOrigin
        )

        // Whole-texture copy — both textures must share the same
        // descriptor (size, pixelFormat, mip count).
        @objc("copyFromTexture:toTexture:")
        pub copyTextureWhole(src: MTLTexture, toTexture: MTLTexture)

        // Fill a byte range of a buffer with `value`. `range` is
        // an `(location, length)` `NSRange`; build with
        // `foundation.makeRange(...)`.
        @objc("fillBuffer:range:value:")
        pub fillBuffer(buffer: MTLBuffer, range: NSRange, value: u8)

        // Compute the mip chain for `texture` from its level-0
        // image. The texture must have been allocated with
        // `mipmapped = true`.
        @objc("generateMipmapsForTexture:")
        pub generateMipmapsForTexture(texture: MTLTexture)

        // Managed-storage synchronisation. macOS-only — pushes any
        // GPU-side writes back to the CPU mapping. No-op (and
        // wasted work) on Apple silicon shared-storage resources.
        @objc("synchronizeResource:")
        pub synchronizeResource(resource: NSObject)
        @objc("synchronizeTexture:slice:level:")
        pub synchronizeTexture(t: MTLTexture, slice: u64, level: u64)

        // Hint the driver to reformat the texture's storage for
        // GPU- or CPU-friendly access. macOS 10.14+ / iOS 12+;
        // a no-op on shared-storage resources.
        @objc("optimizeContentsForGPUAccess:")
        pub optimizeContentsForGPUAccess(resource: NSObject)
        @objc("optimizeContentsForCPUAccess:")
        pub optimizeContentsForCPUAccess(resource: NSObject)

        @objc("setLabel:")
        pub setLabel(s: NSString)
        @objc("pushDebugGroup:")
        pub pushDebugGroup(s: NSString)
        @objc("popDebugGroup")
        pub popDebugGroup()

        // Intra-queue fences (blit has no stage mask either).
        @objc("updateFence:")
        pub updateFence(fence: MTLFence)
        @objc("waitForFence:")
        pub waitForFence(fence: MTLFence)

        @objc("endEncoding")
        pub endEncoding()
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

/// Convenience: build an `MTLSize`. Pass `1` for `height` /
/// `depth` on 1D dispatches, or just `metal.size1D(n)` /
/// `metal.size2D(w, h)` below.
pub fn size(w: u64, h: u64, d: u64): MTLSize {
    let s = new MTLSize()
    s.width = w
    s.height = h
    s.depth = d
    s
}

/// `MTLSize` for a 1D dispatch of `n` threads.
pub fn size1D(n: u64): MTLSize {
    size(n, 1, 1)
}

/// `MTLSize` for a 2D dispatch (`w` × `h`).
pub fn size2D(w: u64, h: u64): MTLSize {
    size(w, h, 1)
}

/// `MTLOrigin` constructor. For 2D textures pass `z = 0`; for 1D
/// pass `y = z = 0`.
pub fn origin(x: u64, y: u64, z: u64): MTLOrigin {
    let o = new MTLOrigin()
    o.x = x
    o.y = y
    o.z = z
    o
}

/// `MTLOrigin` at the texture's top-left corner (all zero).
pub fn originZero(): MTLOrigin {
    origin(0, 0, 0)
}

/// `MTLOrigin` for a 2D texture (`z = 0`).
pub fn origin2D(x: u64, y: u64): MTLOrigin {
    origin(x, y, 0)
}

/// Build an `MTLRegion` from raw `(x, y, z, w, h, d)` components.
pub fn region(x: u64, y: u64, z: u64, w: u64, h: u64, d: u64): MTLRegion {
    let o = new MTLOrigin()
    o.x = x ; o.y = y ; o.z = z
    let s = new MTLSize()
    s.width = w ; s.height = h ; s.depth = d
    let r = new MTLRegion()
    r.origin = o
    r.size = s
    r
}

/// `MTLRegion` covering a 2D `w` × `h` rectangle starting at
/// `(x, y)`. The 99% case for replaceRegion / getBytes on a 2D
/// texture.
pub fn region2D(x: u64, y: u64, w: u64, h: u64): MTLRegion {
    region(x, y, 0, w, h, 1)
}

/// `MTLRegion` covering a 3D `w` × `h` × `d` volume at the
/// origin.
pub fn region3D(x: u64, y: u64, z: u64, w: u64, h: u64, d: u64): MTLRegion {
    region(x, y, z, w, h, d)
}

// ─── Error-aware shader / pipeline wrappers ──────────────────────
//
// Pair-return wrappers around the @objc `…error:` methods on
// MTLDevice. They run the call through `foundation.withObjcError`,
// which hands its per-thread `NSError *` slot address to the
// `error:` argument and reads back whatever the runtime wrote
// into it. On success the second tuple element is `none`; on
// failure the first element's `handle` is typically 0 and the
// second is `some(NSError)` — inspect via
// `e.localizedDescription` / `e.domain` / `e.code`.

/// Compile MSL source into an `MTLLibrary` (default
/// `MTLCompileOptions`).
pub fn newLibraryWithSourceOrError(
    device: MTLDevice, source: NSString
): (MTLLibrary, NSError?) {
    let (h, err) = withObjcError(fn(slot: i64): i64 {
        device.newLibraryWithSource(source, NSObject.wrap(0), slot).handle
    })
    (MTLLibrary.__wrap_handle(h), err)
}

/// Compile + link an `MTLRenderPipelineState` from the descriptor.
pub fn newRenderPipelineStateOrError(
    device: MTLDevice, descriptor: MTLRenderPipelineDescriptor
): (MTLRenderPipelineState, NSError?) {
    let (h, err) = withObjcError(fn(slot: i64): i64 {
        device.newRenderPipelineStateWithDescriptor(descriptor, slot).handle
    })
    (MTLRenderPipelineState.__wrap_handle(h), err)
}

/// Compile + link an `MTLComputePipelineState` from a kernel
/// function.
pub fn newComputePipelineStateOrError(
    device: MTLDevice, function: MTLFunction
): (MTLComputePipelineState, NSError?) {
    let (h, err) = withObjcError(fn(slot: i64): i64 {
        device.newComputePipelineStateWithFunction(function, slot).handle
    })
    (MTLComputePipelineState.__wrap_handle(h), err)
}

// ─── Command-buffer completion / schedule handlers ───────────────
//
// Ergonomic wrappers around `addCompletedHandler:` /
// `addScheduledHandler:`. Each takes an ilang closure of type
// `fn(MTLCommandBuffer)`, packs it into a `void(^)(id)` ObjC
// block via `foundation.makeObjectBlock`, and feeds the block to
// the underlying Metal selector. The runtime invokes the block
// once the command buffer reaches the matching status; the raw
// `id` it hands us is wrapped back into an `MTLCommandBuffer`
// for the user-supplied closure.
//
// The blocks are autoreleased — let them die with the surrounding
// autorelease pool (or hold them in `@autoreleasepool { ... }`
// around the commit if the calling thread doesn't have one).
// Apple may invoke the handler on a non-main thread, so any UI
// work the closure does must hop to the main queue itself.

/// Register a `fn(MTLCommandBuffer)` to fire once `buf` reaches
/// `.completed` (or `.error`) status. Inspect `buf.status()` and
/// `buf.commandBufferError()` inside the closure to distinguish
/// the two.
pub fn onCompleted(buf: MTLCommandBuffer, handler: fn(MTLCommandBuffer)) {
    let block = makeObjectBlock(fn(raw: i64) {
        handler(MTLCommandBuffer.__wrap_handle(raw))
    })
    buf.addCompletedHandler(block)
}

/// Register a `fn(MTLCommandBuffer)` to fire once `buf` reaches
/// `.scheduled` status (GPU has accepted the work; execution
/// hasn't necessarily finished). Useful for triple-buffered
/// frame pacing.
pub fn onScheduled(buf: MTLCommandBuffer, handler: fn(MTLCommandBuffer)) {
    let block = makeObjectBlock(fn(raw: i64) {
        handler(MTLCommandBuffer.__wrap_handle(raw))
    })
    buf.addScheduledHandler(block)
}

// ─── Viewport / scissor convenience constructors ─────────────────

/// Build an `MTLViewport`. Pass `znear = 0.0`, `zfar = 1.0` for the
/// usual depth-clip range, or use `viewport2D(...)` to skip that
/// pair for 2D rendering.
pub fn viewport(
    originX: f64, originY: f64,
    width: f64, height: f64,
    znear: f64, zfar: f64
): MTLViewport {
    let v = new MTLViewport()
    v.originX = originX ; v.originY = originY
    v.width = width ; v.height = height
    v.znear = znear ; v.zfar = zfar
    v
}

/// `MTLViewport` covering a 2D rectangle with the default
/// `znear = 0.0` / `zfar = 1.0` depth-clip range — the casual
/// case when not using depth testing.
pub fn viewport2D(originX: f64, originY: f64, width: f64, height: f64): MTLViewport {
    viewport(originX, originY, width, height, 0.0, 1.0)
}

/// Build an `MTLScissorRect`. All values in pixels, top-left
/// origin (matches Metal's coordinate convention).
pub fn scissorRect(x: u64, y: u64, width: u64, height: u64): MTLScissorRect {
    let r = new MTLScissorRect()
    r.x = x ; r.y = y ; r.width = width ; r.height = height
    r
}
