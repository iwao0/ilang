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
//   * Tier (i):             MTLArgumentEncoder
//                           (function.newArgumentEncoderWithBufferIndex:)
//                           + useResource:/useHeap: residency
//                           declarations on render and compute
//                           encoders; MTLResourceUsage flags
//   * Tier (j):             MTLCaptureManager + MTLCaptureDescriptor +
//                           MTLCaptureScope (Xcode GPU frame capture
//                           / .gputrace export); startCaptureOrError
//                           pair-return wrapper
//
// Surface still intentionally absent (add per-need):
//   * CAMetalLayer / CAMetalDrawable + on-screen presentation
//   * MTLArgumentDescriptor (manual-layout argument encoders)
//   * MTLIndirectCommandBuffer (Tier-2 indirect dispatch)
//   * Tessellation / mesh-shading pipeline knobs
//   * Acceleration structures / ray tracing

use foundation { NSObject, NSString, NSError, NSRange, withObjcError, objcRetain }

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

/// `MTLResourceUsage` — how the shader accesses an argument-buffer
/// resource. Pass the OR of the bits that match the shader
/// declaration to `useResource:usage:` so hazard tracking knows
/// to fence the texture / buffer.
@flags
pub enum MTLResourceUsage: u64 {
    read   = 0x01
    write  = 0x02
    sample = 0x04
}

/// `MTLCaptureDestination` — where a GPU frame capture is sent.
/// `.developerTools` opens it in Xcode (requires the parent
/// process to be running under Xcode or with the
/// `MTL_CAPTURE_ENABLED=1` environment variable);
/// `.gpuTraceDocument` writes a `.gputrace` file the user can
/// open in Xcode later.
pub enum MTLCaptureDestination: u64 {
    developerTools  = 1
    gpuTraceDocument = 2
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

/// Compile MSL source into an `MTLLibrary` (default
/// `MTLCompileOptions`).
pub fn newLibraryWithSourceOrError(
    device: MTLDevice, source: NSString
): (MTLLibrary, NSError?) {
    let (h, err) = withObjcError(fn(slot: i64): i64 {
        // `objcRetain` adds the `+1` the outer `__wrap_handle`
        // expects; the receiver wrapper's deferred scope-exit
        // release (see lower_field's @objc carve-out in
        // ilang-mir) keeps the underlying ObjC object alive
        // through this expression so the retain operates on a
        // live pointer, not a freed one.
        objcRetain(device.newLibraryWithSource(source, NSObject.wrap(0), slot).handle)
    })
    (MTLLibrary.__wrap_handle(h), err)
}

/// Compile + link an `MTLRenderPipelineState` from the descriptor.
pub fn newRenderPipelineStateOrError(
    device: MTLDevice, descriptor: MTLRenderPipelineDescriptor
): (MTLRenderPipelineState, NSError?) {
    let (h, err) = withObjcError(fn(slot: i64): i64 {
        objcRetain(device.newRenderPipelineStateWithDescriptor(descriptor, slot).handle)
    })
    (MTLRenderPipelineState.__wrap_handle(h), err)
}

/// Compile + link an `MTLComputePipelineState` from a kernel
/// function.
pub fn newComputePipelineStateOrError(
    device: MTLDevice, function: MTLFunction
): (MTLComputePipelineState, NSError?) {
    let (h, err) = withObjcError(fn(slot: i64): i64 {
        objcRetain(device.newComputePipelineStateWithFunction(function, slot).handle)
    })
    (MTLComputePipelineState.__wrap_handle(h), err)
}

/// Register a `fn(MTLCommandBuffer)` to fire once `buf` reaches
/// `.completed` (or `.error`) status. Inspect `buf.status()` and
/// `buf.commandBufferError()` inside the closure to distinguish
/// the two.
pub fn onCompleted(buf: MTLCommandBuffer, handler: fn(MTLCommandBuffer)) {
    buf.addCompletedHandler(fn(raw: i64) {
        handler(MTLCommandBuffer.__wrap_handle(raw))
    })
}

/// Register a `fn(MTLCommandBuffer)` to fire once `buf` reaches
/// `.scheduled` status (GPU has accepted the work; execution
/// hasn't necessarily finished). Useful for triple-buffered
/// frame pacing.
pub fn onScheduled(buf: MTLCommandBuffer, handler: fn(MTLCommandBuffer)) {
    buf.addScheduledHandler(fn(raw: i64) {
        handler(MTLCommandBuffer.__wrap_handle(raw))
    })
}

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
