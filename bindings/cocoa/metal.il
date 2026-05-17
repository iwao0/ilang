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

use foundation { NSObject, NSString, NSError, withObjcError }

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
    @objc pub class MTLComputeCommandEncoder : NSObject {
        @objc("setComputePipelineState:")
        pub setComputePipelineState(s: MTLComputePipelineState)

        @objc("setBuffer:offset:atIndex:")
        pub setBuffer(b: MTLBuffer, offset: u64, atIndex: u64)
        @objc("setBytes:length:atIndex:")
        pub setBytes(bytes: i64, length: u64, atIndex: u64)
        @objc("setTexture:atIndex:")
        pub setTexture(t: MTLTexture, atIndex: u64)

        @objc("dispatchThreads:threadsPerThreadgroup:")
        pub dispatchThreads(
            threads: MTLSize, threadsPerThreadgroup: MTLSize
        )
        @objc("dispatchThreadgroups:threadsPerThreadgroup:")
        pub dispatchThreadgroups(
            threadgroups: MTLSize, threadsPerThreadgroup: MTLSize
        )

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
