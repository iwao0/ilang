/// wgpu-native (WebGPU) minimal bindings — PoC subset.
///
/// Pinned to wgpu-native v29.0.0.0 (webgpu.h). Only the surface
/// needed to clear a surface and draw a WGSL triangle is bound; the
/// header is large, so this grows on demand.
///
/// ABI notes (v29):
/// - Opaque object handles (`WGPUInstanceImpl*` …) are pointer-sized
///   `@handle` types.
/// - Strings are `WGPUStringView { const char* data; size_t len }`
///   (16 B, passed in 2 regs). In callback signatures we splay it
///   into two `i64`s so the C calling convention lines up without a
///   by-value struct param.
/// - `wgpuInstanceRequestAdapter` / `wgpuAdapterRequestDevice` return
///   `WGPUFuture { uint64_t id }` (8 B → 1 reg ≡ `u64`) and take their
///   `CallbackInfo` *by value* (40 B → passed indirectly per AAPCS64).
/// - Library: bare name "wgpu_native" → `libwgpu_native.dylib`. Point
///   the loader at it via `DYLD_LIBRARY_PATH` (PoC) — a real build
///   would resolve a bundled copy per OS.

use std.ffi { cstrFromString }

/// `WGPUTextureFormat` — full v29 enumeration. Discriminants
/// match `webgpu.h`; cast to / from `u32` is identity.
/// `undefined` is the "no value passed" sentinel.
pub enum TextureFormat: u32 {
    undefined            = 0x00
    r8Unorm              = 0x01
    r8Snorm              = 0x02
    r8Uint               = 0x03
    r8Sint               = 0x04
    r16Unorm             = 0x05
    r16Snorm             = 0x06
    r16Uint              = 0x07
    r16Sint              = 0x08
    r16Float             = 0x09
    rg8Unorm             = 0x0A
    rg8Snorm             = 0x0B
    rg8Uint              = 0x0C
    rg8Sint              = 0x0D
    r32Float             = 0x0E
    r32Uint              = 0x0F
    r32Sint              = 0x10
    rg16Unorm            = 0x11
    rg16Snorm            = 0x12
    rg16Uint             = 0x13
    rg16Sint             = 0x14
    rg16Float            = 0x15
    rgba8Unorm           = 0x16
    rgba8UnormSrgb       = 0x17
    rgba8Snorm           = 0x18
    rgba8Uint            = 0x19
    rgba8Sint            = 0x1A
    bgra8Unorm           = 0x1B
    bgra8UnormSrgb       = 0x1C
    rgb10a2Uint          = 0x1D
    rgb10a2Unorm         = 0x1E
    rg11b10Ufloat        = 0x1F
    rgb9e5Ufloat         = 0x20
    rg32Float            = 0x21
    rg32Uint             = 0x22
    rg32Sint             = 0x23
    rgba16Unorm          = 0x24
    rgba16Snorm          = 0x25
    rgba16Uint           = 0x26
    rgba16Sint           = 0x27
    rgba16Float          = 0x28
    rgba32Float          = 0x29
    rgba32Uint           = 0x2A
    rgba32Sint           = 0x2B
    stencil8             = 0x2C
    depth16Unorm         = 0x2D
    depth24Plus          = 0x2E
    depth24PlusStencil8  = 0x2F
    depth32Float         = 0x30
    depth32FloatStencil8 = 0x31
    bc1RgbaUnorm         = 0x32
    bc1RgbaUnormSrgb     = 0x33
    bc2RgbaUnorm         = 0x34
    bc2RgbaUnormSrgb     = 0x35
    bc3RgbaUnorm         = 0x36
    bc3RgbaUnormSrgb     = 0x37
    bc4RUnorm            = 0x38
    bc4RSnorm            = 0x39
    bc5RgUnorm           = 0x3A
    bc5RgSnorm           = 0x3B
    bc6hRgbUfloat        = 0x3C
    bc6hRgbFloat         = 0x3D
    bc7RgbaUnorm         = 0x3E
    bc7RgbaUnormSrgb     = 0x3F
    etc2Rgb8Unorm        = 0x40
    etc2Rgb8UnormSrgb    = 0x41
    etc2Rgb8a1Unorm      = 0x42
    etc2Rgb8a1UnormSrgb  = 0x43
    etc2Rgba8Unorm       = 0x44
    etc2Rgba8UnormSrgb   = 0x45
    eacR11Unorm          = 0x46
    eacR11Snorm          = 0x47
    eacRg11Unorm         = 0x48
    eacRg11Snorm         = 0x49
    astc4x4Unorm         = 0x4A
    astc4x4UnormSrgb     = 0x4B
    astc5x4Unorm         = 0x4C
    astc5x4UnormSrgb     = 0x4D
    astc5x5Unorm         = 0x4E
    astc5x5UnormSrgb     = 0x4F
    astc6x5Unorm         = 0x50
    astc6x5UnormSrgb     = 0x51
    astc6x6Unorm         = 0x52
    astc6x6UnormSrgb     = 0x53
    astc8x5Unorm         = 0x54
    astc8x5UnormSrgb     = 0x55
    astc8x6Unorm         = 0x56
    astc8x6UnormSrgb     = 0x57
    astc8x8Unorm         = 0x58
    astc8x8UnormSrgb     = 0x59
    astc10x5Unorm        = 0x5A
    astc10x5UnormSrgb    = 0x5B
    astc10x6Unorm        = 0x5C
    astc10x6UnormSrgb    = 0x5D
    astc10x8Unorm        = 0x5E
    astc10x8UnormSrgb    = 0x5F
    astc10x10Unorm       = 0x60
    astc10x10UnormSrgb   = 0x61
    astc12x10Unorm       = 0x62
    astc12x10UnormSrgb   = 0x63
    astc12x12Unorm       = 0x64
    astc12x12UnormSrgb   = 0x65
}

@extern(C, "wgpu_native") {
    // ─── opaque object handles ────────────────────────────────
    @handle pub struct WGPUInstance {}
    @handle pub struct WGPUAdapter {}
    @handle pub struct WGPUDevice {}
    @handle pub struct WGPUQueue {}
    @handle pub struct WGPUSurface {}
    @handle pub struct WGPUTexture {}
    @handle pub struct WGPUTextureView {}
    @handle pub struct WGPUCommandEncoder {}
    @handle pub struct WGPUCommandBuffer {}
    @handle pub struct WGPURenderPassEncoder {}
    @handle pub struct WGPUShaderModule {}
    @handle pub struct WGPURenderPipeline {}

    // ─── common value structs ─────────────────────────────────
    // WGPUStringView { const char* data; size_t length }. Empty
    // label = { 0, 0 }.
    pub struct WGPUStringView {
        data:   *void
        length: u64
    }
    pub struct WGPUChainedStruct {
        next:  *void
        sType: u32
    }
    pub struct WGPUColor {
        r: f64
        g: f64
        b: f64
        a: f64
    }

    // ─── async callback plumbing ──────────────────────────────
    // WGPUCallbackMode: WaitAnyOnly=1, AllowProcessEvents=2,
    // AllowSpontaneous=3.
    //
    // `callback` shape mirrors
    //   void(WGPURequestAdapterStatus status, WGPUAdapter adapter,
    //        WGPUStringView message, void* ud1, void* ud2)
    // with `message` splayed into (data, len) i64s.
    pub struct WGPURequestAdapterCallbackInfo {
        nextInChain: *void
        mode:        u32
        callback:    fn(i32, i64, i64, i64, i64, i64)
        userdata1:   *void
        userdata2:   *void
    }

    // All enum/bool fields are 4 B; `compatibleSurface` makes the
    // chosen adapter able to present to that surface (required before
    // wgpuSurfaceConfigure, else "invalid device for surface").
    // featureLevel/powerPreference/backendType Undefined = 0.
    pub struct WGPURequestAdapterOptions {
        nextInChain:          *void
        featureLevel:         u32
        powerPreference:      u32
        forceFallbackAdapter: u32     // WGPUBool
        backendType:          u32
        compatibleSurface:    WGPUSurface
    }

    // Same layout; callback is
    //   void(WGPURequestDeviceStatus, WGPUDevice, WGPUStringView, *, *)
    pub struct WGPURequestDeviceCallbackInfo {
        nextInChain: *void
        mode:        u32
        callback:    fn(i32, i64, i64, i64, i64, i64)
        userdata1:   *void
        userdata2:   *void
    }

    // ─── entry points ─────────────────────────────────────────
    // `descriptor` / `options` are nullable — pass `0 as *void`.
    // ─── logging (wgpu.h native extension) ────────────────────
    // WGPULogLevel: Off=0 Error=1 Warn=2 Info=3 Debug=4 Trace=5.
    // callback: void(WGPULogLevel, WGPUStringView message, void*)
    // → (level i32, msg.data i64, msg.len i64, userdata i64).
    @lib pub fn wgpuSetLogLevel(level: u32)
    @lib pub fn wgpuSetLogCallback(
        callback: fn(i32, i64, i64, i64), userdata: *void
    )

    @lib pub fn wgpuCreateInstance(descriptor: *void): WGPUInstance
    @lib pub fn wgpuInstanceRequestAdapter(
        instance: WGPUInstance,
        options: *void,
        callbackInfo: WGPURequestAdapterCallbackInfo
    ): u64
    @lib pub fn wgpuInstanceProcessEvents(instance: WGPUInstance)

    @lib pub fn wgpuAdapterRequestDevice(
        adapter: WGPUAdapter,
        descriptor: *void,
        callbackInfo: WGPURequestDeviceCallbackInfo
    ): u64
    @lib pub fn wgpuDeviceGetQueue(device: WGPUDevice): WGPUQueue

    // ─── surface ──────────────────────────────────────────────
    // WGPUSType_SurfaceSourceMetalLayer = 4. Chain this off the
    // descriptor's nextInChain for the macOS surface.
    pub struct WGPUSurfaceSourceMetalLayer {
        chain: WGPUChainedStruct
        layer: *void              // CAMetalLayer*
    }
    // WGPUSType_SurfaceSourceWindowsHWND = 5. Chain this off the
    // descriptor's nextInChain for the Windows surface. `hinstance`
    // is the module HINSTANCE, `hwnd` the window handle — both pulled
    // from SDL's `SysWMinfo` (handle3 / handle1 respectively).
    pub struct WGPUSurfaceSourceWindowsHWND {
        chain:     WGPUChainedStruct
        hinstance: *void          // HINSTANCE
        hwnd:      *void          // HWND
    }
    // WGPUSType_SurfaceSourceXlibWindow = 6. Linux/X11: `display` is
    // the X `Display*`, `window` the XID (a 64-bit value). Both come
    // from SDL's `SysWMinfo` (handle1 / handle2) when subsystem == x11.
    pub struct WGPUSurfaceSourceXlibWindow {
        chain:   WGPUChainedStruct
        display: *void            // Display*
        window:  u64              // X Window XID
    }
    // WGPUSType_SurfaceSourceWaylandSurface = 7. Linux/Wayland:
    // `display` is the `wl_display*`, `surface` the `wl_surface*`.
    // From SysWMinfo (handle1 / handle2) when subsystem == wayland.
    pub struct WGPUSurfaceSourceWaylandSurface {
        chain:   WGPUChainedStruct
        display: *void            // wl_display*
        surface: *void            // wl_surface*
    }
    pub struct WGPUSurfaceDescriptor {
        nextInChain: *void        // WGPUChainedStruct*
        label:       WGPUStringView
    }
    // format = WGPUTextureFormat (enum, 4B). usage = WGPUTextureUsage
    // (WGPUFlags = uint64). RenderAttachment = 0x10.
    pub struct WGPUSurfaceConfiguration {
        nextInChain:     *void
        device:          WGPUDevice
        format:          u32
        usage:           u64
        width:           u32
        height:          u32
        viewFormatCount: u64
        viewFormats:     *void
        alphaMode:       u32
        presentMode:     u32
    }
    pub struct WGPUSurfaceTexture {
        nextInChain: *void
        texture:     WGPUTexture
        status:      u32          // SuccessOptimal = 1
    }

    // Out-param: wgpu fills the arrays. The element pointers are
    // owned by wgpu (free with wgpuSurfaceCapabilitiesFreeMembers).
    pub struct WGPUSurfaceCapabilities {
        nextInChain:      *void
        usages:           u64
        formatCount:      u64
        formats:          *void    // WGPUTextureFormat const*
        presentModeCount: u64
        presentModes:     *void    // WGPUPresentMode const*
        alphaModeCount:   u64
        alphaModes:       *void    // WGPUCompositeAlphaMode const*
    }
    @lib pub fn wgpuSurfaceGetCapabilities(
        surface: WGPUSurface, adapter: WGPUAdapter,
        capabilities: *WGPUSurfaceCapabilities
    ): u32

    @lib pub fn wgpuInstanceCreateSurface(
        instance: WGPUInstance, descriptor: *WGPUSurfaceDescriptor
    ): WGPUSurface
    @lib pub fn wgpuSurfaceConfigure(
        surface: WGPUSurface, config: *WGPUSurfaceConfiguration
    )
    @lib pub fn wgpuSurfaceGetCurrentTexture(
        surface: WGPUSurface, surfaceTexture: *WGPUSurfaceTexture
    )
    @lib pub fn wgpuTextureCreateView(
        texture: WGPUTexture, descriptor: *void
    ): WGPUTextureView
    @lib pub fn wgpuSurfacePresent(surface: WGPUSurface)

    // ─── command encoding / render pass ───────────────────────
    // WGPULoadOp_Clear = 2, WGPUStoreOp_Store = 1.
    pub struct WGPURenderPassColorAttachment {
        nextInChain:   *void
        view:          WGPUTextureView
        depthSlice:    u32        // WGPU_DEPTH_SLICE_UNDEFINED = 0xFFFFFFFF
        resolveTarget: WGPUTextureView   // nullable
        loadOp:        u32
        storeOp:       u32
        clearValue:    WGPUColor
    }
    pub struct WGPURenderPassDescriptor {
        nextInChain:            *void
        label:                  WGPUStringView
        colorAttachmentCount:   u64
        colorAttachments:       *WGPURenderPassColorAttachment
        depthStencilAttachment: *void
        occlusionQuerySet:      *void
        timestampWrites:        *void
    }

    @lib pub fn wgpuDeviceCreateCommandEncoder(
        device: WGPUDevice, descriptor: *void
    ): WGPUCommandEncoder
    @lib pub fn wgpuCommandEncoderBeginRenderPass(
        commandEncoder: WGPUCommandEncoder,
        descriptor: *WGPURenderPassDescriptor
    ): WGPURenderPassEncoder
    @lib pub fn wgpuRenderPassEncoderEnd(encoder: WGPURenderPassEncoder)
    @lib pub fn wgpuCommandEncoderFinish(
        commandEncoder: WGPUCommandEncoder, descriptor: *void
    ): WGPUCommandBuffer
    // `commands` is a `WGPUCommandBuffer const*` (array). Pass a
    // 1-element `WGPUCommandBuffer[]` (T[] → *T coercion).
    @lib pub fn wgpuQueueSubmit(
        queue: WGPUQueue, commandCount: u64, commands: *WGPUCommandBuffer
    )

    // ─── shader module (WGSL) ─────────────────────────────────
    // WGPUSType_ShaderSourceWGSL = 2. Chain off the descriptor's
    // nextInChain. `code` / entry-point StringViews can use
    // length = WGPU_STRLEN (SIZE_MAX) to let wgpu strlen the data.
    pub struct WGPUShaderSourceWGSL {
        chain: WGPUChainedStruct
        code:  WGPUStringView
    }
    pub struct WGPUShaderModuleDescriptor {
        nextInChain: *void
        label:       WGPUStringView
    }
    @lib pub fn wgpuDeviceCreateShaderModule(
        device: WGPUDevice, descriptor: *WGPUShaderModuleDescriptor
    ): WGPUShaderModule

    // ─── render pipeline ──────────────────────────────────────
    pub struct WGPUColorTargetState {
        nextInChain: *void
        format:      u32
        blend:       *void    // WGPUBlendState* (nullable)
        writeMask:   u64      // WGPUColorWriteMask flags; All = 0xF
    }
    pub struct WGPUVertexState {
        nextInChain:   *void
        module:        WGPUShaderModule
        entryPoint:    WGPUStringView
        constantCount: u64
        constants:     *void
        bufferCount:   u64
        buffers:       *void
    }
    pub struct WGPUPrimitiveState {
        nextInChain:      *void
        topology:         u32  // TriangleList = 4
        stripIndexFormat: u32
        frontFace:        u32  // CCW = 1
        cullMode:         u32  // None = 1
        unclippedDepth:   u32  // WGPUBool
    }
    pub struct WGPUMultisampleState {
        nextInChain:            *void
        count:                  u32
        mask:                   u32
        alphaToCoverageEnabled: u32  // WGPUBool
    }
    pub struct WGPUFragmentState {
        nextInChain:   *void
        module:        WGPUShaderModule
        entryPoint:    WGPUStringView
        constantCount: u64
        constants:     *void
        targetCount:   u64
        targets:       *WGPUColorTargetState
    }
    pub struct WGPURenderPipelineDescriptor {
        nextInChain:    *void
        label:          WGPUStringView
        layout:         *void              // WGPUPipelineLayout (nullable = auto)
        vertex:         WGPUVertexState
        primitive:      WGPUPrimitiveState
        depthStencil:   *void              // WGPUDepthStencilState* (nullable)
        multisample:    WGPUMultisampleState
        fragment:       *WGPUFragmentState  // (nullable)
    }
    @lib pub fn wgpuDeviceCreateRenderPipeline(
        device: WGPUDevice, descriptor: *WGPURenderPipelineDescriptor
    ): WGPURenderPipeline

    @lib pub fn wgpuRenderPassEncoderSetPipeline(
        encoder: WGPURenderPassEncoder, pipeline: WGPURenderPipeline
    )
    @lib pub fn wgpuRenderPassEncoderDraw(
        encoder: WGPURenderPassEncoder,
        vertexCount: u32, instanceCount: u32,
        firstVertex: u32, firstInstance: u32
    )

    // ─── offscreen textures + buffer readback ─────────────────
    //
    // The libs/gui/linux GpuView path renders into a texture
    // (rather than a surface) and copies the pixels back to the
    // CPU so GTK / Cairo can composite the result inside the
    // window's content area. Same primitives also work for any
    // other "render to image" use case.
    @handle pub struct WGPUBuffer {}
    @handle pub struct WGPUBindGroup {}
    @handle pub struct WGPUBindGroupLayout {}
    @handle pub struct WGPUPipelineLayout {}
    @handle pub struct WGPUSampler {}
    @handle pub struct WGPUComputePipeline {}
    @handle pub struct WGPUComputePassEncoder {}

    pub struct WGPUExtent3D {
        width:              u32
        height:             u32
        depthOrArrayLayers: u32
    }
    pub struct WGPUOrigin3D {
        x: u32
        y: u32
        z: u32
    }
    // WGPUTextureDimension: _1D=1, _2D=2, _3D=3. Usage is a 64-bit
    // bitfield (RenderAttachment = 0x10, CopySrc = 0x01, …).
    pub struct WGPUTextureDescriptor {
        nextInChain:     *void
        label:           WGPUStringView
        usage:           u64
        dimension:       u32
        size:            WGPUExtent3D
        format:          u32
        mipLevelCount:   u32
        sampleCount:     u32
        viewFormatCount: u64
        viewFormats:     *void
    }
    @lib pub fn wgpuDeviceCreateTexture(
        device: WGPUDevice, descriptor: *WGPUTextureDescriptor
    ): WGPUTexture

    // Buffer usage flags (u64): MapRead = 0x01, MapWrite = 0x02,
    // CopySrc = 0x04, CopyDst = 0x08. mappedAtCreation is WGPUBool.
    pub struct WGPUBufferDescriptor {
        nextInChain:      *void
        label:            WGPUStringView
        usage:            u64
        size:             u64
        mappedAtCreation: u32
    }
    @lib pub fn wgpuDeviceCreateBuffer(
        device: WGPUDevice, descriptor: *WGPUBufferDescriptor
    ): WGPUBuffer

    // Texture→buffer copy plumbing. bytesPerRow MUST be a multiple
    // of 256 (WebGPU spec); pad the row stride if needed.
    pub struct WGPUTexelCopyBufferLayout {
        offset:       u64
        bytesPerRow:  u32
        rowsPerImage: u32
    }
    pub struct WGPUTexelCopyTextureInfo {
        texture:  WGPUTexture
        mipLevel: u32
        origin:   WGPUOrigin3D
        aspect:   u32             // WGPUTextureAspect_All = 1
    }
    pub struct WGPUTexelCopyBufferInfo {
        layout: WGPUTexelCopyBufferLayout
        buffer: WGPUBuffer
    }
    @lib pub fn wgpuCommandEncoderCopyTextureToBuffer(
        encoder:     WGPUCommandEncoder,
        source:      *WGPUTexelCopyTextureInfo,
        destination: *WGPUTexelCopyBufferInfo,
        copySize:    *WGPUExtent3D
    )

    // Async map. Callback shape:
    //   void(WGPUMapAsyncStatus status, WGPUStringView message,
    //        void* userdata1, void* userdata2)
    // → (status: i32, msg.data: i64, msg.len: i64, ud1: i64, ud2: i64).
    pub struct WGPUBufferMapCallbackInfo {
        nextInChain: *void
        mode:        u32                            // CallbackMode
        callback:    fn(i32, i64, i64, i64, i64)
        userdata1:   *void
        userdata2:   *void
    }
    @lib pub fn wgpuBufferMapAsync(
        buffer: WGPUBuffer, mode: u64, offset: u64, size: u64,
        callbackInfo: WGPUBufferMapCallbackInfo
    ): u64
    // Mapped pointer is owned by wgpu until the matching unmap.
    @lib pub fn wgpuBufferGetConstMappedRange(
        buffer: WGPUBuffer, offset: u64, size: u64
    ): *const void
    @lib pub fn wgpuBufferUnmap(buffer: WGPUBuffer)
    @lib pub fn wgpuBufferRelease(buffer: WGPUBuffer)

    // wgpu-native extension. `wait` = 1 blocks until pending work
    // and any registered map callbacks complete; pass null for the
    // submission-index pointer to poll on every queued submit.
    @lib pub fn wgpuDevicePoll(
        device: WGPUDevice, wait: u32, submissionIndex: *const u64
    ): u32

    // ─── buffer reads / writes / introspection ────────────────
    //
    // `wgpuQueueWriteBuffer` is the no-staging fast path for small
    // / single-shot uploads (uniform writes, per-frame transform
    // matrices). For larger data prefer mapAsync + a CopyDst buffer.
    @lib pub fn wgpuQueueWriteBuffer(
        queue: WGPUQueue, buffer: WGPUBuffer, bufferOffset: u64,
        data: *const void, size: u64
    )
    // CPU → texture upload. `dataLayout.bytesPerRow` must be a
    // multiple of 256 (same WebGPU spec rule as
    // CopyTextureToBuffer); pad rows before calling.
    @lib pub fn wgpuQueueWriteTexture(
        queue:       WGPUQueue,
        destination: *WGPUTexelCopyTextureInfo,
        data:        *const void,
        dataSize:    u64,
        dataLayout:  *WGPUTexelCopyBufferLayout,
        writeSize:   *WGPUExtent3D
    )
    @lib pub fn wgpuBufferGetSize(buffer: WGPUBuffer): u64
    // WGPUBufferMapState: Unmapped=1, Pending=2, Mapped=3.
    @lib pub fn wgpuBufferGetMapState(buffer: WGPUBuffer): u32
    // Same flag set as WGPUBufferDescriptor.usage.
    @lib pub fn wgpuBufferGetUsage(buffer: WGPUBuffer): u64
    // Releases GPU resources eagerly without dropping the host-side
    // wrapper. Subsequent operations on the buffer fail (validation
    // error in wgpu) — call only when you know no in-flight command
    // can still reference it.
    @lib pub fn wgpuBufferDestroy(buffer: WGPUBuffer)

    // ─── vertex / index buffer plumbing ───────────────────────
    //
    // `WGPUVertexFormat` discriminants (raw u32): Uint8x2=0x01,
    // Uint8x4=0x03, Sint8x2=0x05, Snorm8x4=0x0A, Unorm16x2=0x10,
    // Float32=0x1C, Float32x2=0x1D, Float32x3=0x1E, Float32x4=0x1F.
    // `WGPUVertexStepMode`: Undefined=0, Vertex=1, Instance=2.
    pub struct WGPUVertexAttribute {
        nextInChain:    *void
        format:         u32        // WGPUVertexFormat
        offset:         u64
        shaderLocation: u32
    }
    pub struct WGPUVertexBufferLayout {
        nextInChain:    *void
        stepMode:       u32        // WGPUVertexStepMode
        arrayStride:    u64
        attributeCount: u64
        attributes:     *WGPUVertexAttribute
    }

    // `slot` = vertex-buffer binding index in the pipeline's
    // `vertex.buffers[]`. `buffer` may be null to unbind. `size`
    // = WGPU_WHOLE_SIZE (= u64::MAX) means "use the rest of the
    // buffer from `offset`".
    @lib pub fn wgpuRenderPassEncoderSetVertexBuffer(
        encoder: WGPURenderPassEncoder, slot: u32,
        buffer:  WGPUBuffer, offset: u64, size: u64
    )
    // WGPUIndexFormat: Undefined=0, Uint16=1, Uint32=2.
    @lib pub fn wgpuRenderPassEncoderSetIndexBuffer(
        encoder: WGPURenderPassEncoder, buffer: WGPUBuffer,
        format: u32, offset: u64, size: u64
    )
    // `baseVertex` is signed — lets you reuse an index buffer
    // across geometry sub-ranges by shifting the per-vertex
    // lookup.
    @lib pub fn wgpuRenderPassEncoderDrawIndexed(
        encoder:       WGPURenderPassEncoder,
        indexCount:    u32, instanceCount: u32,
        firstIndex:    u32, baseVertex:    i32,
        firstInstance: u32
    )

    // ─── render-pass viewport / scissor ───────────────────────
    @lib pub fn wgpuRenderPassEncoderSetViewport(
        encoder: WGPURenderPassEncoder,
        x: f32, y: f32, width: f32, height: f32,
        minDepth: f32, maxDepth: f32
    )
    @lib pub fn wgpuRenderPassEncoderSetScissorRect(
        encoder: WGPURenderPassEncoder,
        x: u32, y: u32, width: u32, height: u32
    )

    // ─── bind groups + pipeline layouts + samplers ────────────
    //
    // The four `*BindingLayout` value structs always appear
    // nested inside a `WGPUBindGroupLayoutEntry`; exactly one of
    // them is non-zero (the others stay zero-initialised). Enum
    // discriminants follow `webgpu.h`:
    //   WGPUBufferBindingType:  Undefined=0, Uniform=1,
    //                           Storage=2, ReadOnlyStorage=3
    //   WGPUSamplerBindingType: Undefined=0, Filtering=1,
    //                           NonFiltering=2, Comparison=3
    //   WGPUTextureSampleType:  Undefined=0, Float=1,
    //                           UnfilterableFloat=2, Depth=3,
    //                           Sint=4, Uint=5
    //   WGPUTextureViewDimension: Undefined=0, _1D=1, _2D=2,
    //                           _2DArray=3, Cube=4, CubeArray=5,
    //                           _3D=6
    //   WGPUStorageTextureAccess: Undefined=0, WriteOnly=1,
    //                             ReadOnly=2, ReadWrite=3
    //   WGPUAddressMode:        Undefined=0, ClampToEdge=1,
    //                           Repeat=2, MirrorRepeat=3
    //   WGPUFilterMode:         Undefined=0, Nearest=1, Linear=2
    //   WGPUMipmapFilterMode:   Undefined=0, Nearest=1, Linear=2
    //   WGPUCompareFunction:    Undefined=0, Never=1, Less=2,
    //                           Equal=3, LessEqual=4, Greater=5,
    //                           NotEqual=6, GreaterEqual=7,
    //                           Always=8
    //   WGPUShaderStage flags:  None=0, Vertex=1, Fragment=2,
    //                           Compute=4

    pub struct WGPUBufferBindingLayout {
        nextInChain:      *void
        type:             u32      // WGPUBufferBindingType
        hasDynamicOffset: u32      // WGPUBool
        minBindingSize:   u64
    }
    pub struct WGPUSamplerBindingLayout {
        nextInChain: *void
        type:        u32           // WGPUSamplerBindingType
    }
    pub struct WGPUTextureBindingLayout {
        nextInChain:   *void
        sampleType:    u32         // WGPUTextureSampleType
        viewDimension: u32         // WGPUTextureViewDimension
        multisampled:  u32         // WGPUBool
    }
    pub struct WGPUStorageTextureBindingLayout {
        nextInChain:   *void
        access:        u32         // WGPUStorageTextureAccess
        format:        u32         // WGPUTextureFormat
        viewDimension: u32         // WGPUTextureViewDimension
    }
    pub struct WGPUBindGroupLayoutEntry {
        nextInChain:      *void
        binding:          u32
        visibility:       u64      // WGPUShaderStage flag set
        bindingArraySize: u32      // 0 = scalar; >0 = array binding
        buffer:           WGPUBufferBindingLayout
        sampler:          WGPUSamplerBindingLayout
        texture:          WGPUTextureBindingLayout
        storageTexture:   WGPUStorageTextureBindingLayout
    }
    pub struct WGPUBindGroupLayoutDescriptor {
        nextInChain: *void
        label:       WGPUStringView
        entryCount:  u64
        entries:     *WGPUBindGroupLayoutEntry
    }

    pub struct WGPUBindGroupEntry {
        nextInChain: *void
        binding:     u32
        // For BUFFER bindings: set `buffer` (+ `offset` / `size`)
        // and leave `sampler` / `textureView` null. For SAMPLER
        // / TEXTURE bindings: clear `buffer` and set the matching
        // handle. `size` = u64::MAX means "to end of buffer".
        buffer:      WGPUBuffer
        offset:      u64
        size:        u64
        sampler:     WGPUSampler
        textureView: WGPUTextureView
    }
    pub struct WGPUBindGroupDescriptor {
        nextInChain: *void
        label:       WGPUStringView
        layout:      WGPUBindGroupLayout
        entryCount:  u64
        entries:     *WGPUBindGroupEntry
    }

    pub struct WGPUPipelineLayoutDescriptor {
        nextInChain:          *void
        label:                WGPUStringView
        bindGroupLayoutCount: u64
        bindGroupLayouts:     *WGPUBindGroupLayout
    }
    pub struct WGPUSamplerDescriptor {
        nextInChain:   *void
        label:         WGPUStringView
        addressModeU:  u32     // WGPUAddressMode
        addressModeV:  u32
        addressModeW:  u32
        magFilter:     u32     // WGPUFilterMode
        minFilter:     u32
        mipmapFilter:  u32     // WGPUMipmapFilterMode
        lodMinClamp:   f32
        lodMaxClamp:   f32
        compare:       u32     // WGPUCompareFunction
        maxAnisotropy: u16
    }

    @lib pub fn wgpuDeviceCreateBindGroupLayout(
        device: WGPUDevice, descriptor: *WGPUBindGroupLayoutDescriptor
    ): WGPUBindGroupLayout
    @lib pub fn wgpuDeviceCreateBindGroup(
        device: WGPUDevice, descriptor: *WGPUBindGroupDescriptor
    ): WGPUBindGroup
    @lib pub fn wgpuDeviceCreatePipelineLayout(
        device: WGPUDevice, descriptor: *WGPUPipelineLayoutDescriptor
    ): WGPUPipelineLayout
    // `descriptor` may be null — wgpu then uses the default
    // sampler (ClampToEdge / Nearest / no compare).
    @lib pub fn wgpuDeviceCreateSampler(
        device: WGPUDevice, descriptor: *WGPUSamplerDescriptor
    ): WGPUSampler

    // Pull a single bind-group layout out of an auto-layout
    // pipeline (one created with `layout = NULL`). `groupIndex`
    // matches the `@group(N)` annotation in WGSL.
    @lib pub fn wgpuRenderPipelineGetBindGroupLayout(
        pipeline: WGPURenderPipeline, groupIndex: u32
    ): WGPUBindGroupLayout

    // `group` may be null to unbind. `dynamicOffsetCount` / -s
    // are zero / null unless any binding in `group`'s layout was
    // declared `hasDynamicOffset = true`.
    @lib pub fn wgpuRenderPassEncoderSetBindGroup(
        encoder:             WGPURenderPassEncoder,
        groupIndex:          u32,
        group:               WGPUBindGroup,
        dynamicOffsetCount:  u64,
        dynamicOffsets:      *const u32
    )

    @lib pub fn wgpuBindGroupRelease(bindGroup: WGPUBindGroup)
    @lib pub fn wgpuBindGroupLayoutRelease(layout: WGPUBindGroupLayout)
    @lib pub fn wgpuPipelineLayoutRelease(layout: WGPUPipelineLayout)
    @lib pub fn wgpuSamplerRelease(sampler: WGPUSampler)

    // ─── compute pipeline + compute pass ──────────────────────
    //
    // Symmetric with the render pipeline / render pass shape:
    //   create pipeline → begin pass → set pipeline → set bind
    //   groups → dispatchWorkgroups (×N) → end → submit.
    //
    // `WGPUComputeState` mirrors `WGPUVertexState` (one shader
    // module + entry point + override constants); compute
    // pipelines carry exactly one stage so there's no fragment
    // counterpart.

    pub struct WGPUComputeState {
        nextInChain:   *void
        module:        WGPUShaderModule
        entryPoint:    WGPUStringView
        constantCount: u64
        constants:     *void          // WGPUConstantEntry const*; null = none
    }
    pub struct WGPUComputePipelineDescriptor {
        nextInChain: *void
        label:       WGPUStringView
        // `layout` may be null — wgpu auto-derives a pipeline
        // layout from the shader module's reflection just like
        // the render-pipeline path.
        layout:      WGPUPipelineLayout
        compute:     WGPUComputeState
    }
    pub struct WGPUComputePassDescriptor {
        nextInChain:     *void
        label:           WGPUStringView
        timestampWrites: *void        // WGPUPassTimestampWrites const*
    }

    @lib pub fn wgpuDeviceCreateComputePipeline(
        device: WGPUDevice, descriptor: *WGPUComputePipelineDescriptor
    ): WGPUComputePipeline
    @lib pub fn wgpuComputePipelineGetBindGroupLayout(
        pipeline: WGPUComputePipeline, groupIndex: u32
    ): WGPUBindGroupLayout

    // `descriptor` may be null for a plain pass with no
    // timestamp queries.
    @lib pub fn wgpuCommandEncoderBeginComputePass(
        encoder: WGPUCommandEncoder, descriptor: *WGPUComputePassDescriptor
    ): WGPUComputePassEncoder
    @lib pub fn wgpuComputePassEncoderSetPipeline(
        encoder: WGPUComputePassEncoder, pipeline: WGPUComputePipeline
    )
    @lib pub fn wgpuComputePassEncoderSetBindGroup(
        encoder:            WGPUComputePassEncoder,
        groupIndex:         u32,
        group:              WGPUBindGroup,
        dynamicOffsetCount: u64,
        dynamicOffsets:     *const u32
    )
    @lib pub fn wgpuComputePassEncoderDispatchWorkgroups(
        encoder: WGPUComputePassEncoder,
        workgroupCountX: u32, workgroupCountY: u32, workgroupCountZ: u32
    )
    // Indirect dispatch reads `[x, y, z]` u32 triples out of
    // `indirectBuffer` at `indirectOffset` — useful when the
    // workgroup count is itself computed on the GPU.
    @lib pub fn wgpuComputePassEncoderDispatchWorkgroupsIndirect(
        encoder: WGPUComputePassEncoder,
        indirectBuffer: WGPUBuffer, indirectOffset: u64
    )
    @lib pub fn wgpuComputePassEncoderEnd(encoder: WGPUComputePassEncoder)

    @lib pub fn wgpuComputePipelineRelease(pipeline: WGPUComputePipeline)
    @lib pub fn wgpuComputePassEncoderRelease(encoder: WGPUComputePassEncoder)

    // ─── release (avoid leaks across frames) ──────────────────
    @lib pub fn wgpuTextureViewRelease(view: WGPUTextureView)
    @lib pub fn wgpuTextureRelease(texture: WGPUTexture)
    @lib pub fn wgpuRenderPassEncoderRelease(encoder: WGPURenderPassEncoder)
    @lib pub fn wgpuCommandEncoderRelease(encoder: WGPUCommandEncoder)
    @lib pub fn wgpuCommandBufferRelease(commandBuffer: WGPUCommandBuffer)
    @lib pub fn wgpuShaderModuleRelease(shaderModule: WGPUShaderModule)
    @lib pub fn wgpuRenderPipelineRelease(pipeline: WGPURenderPipeline)

    // ═══════════════════════════════════════════════════════════
    //  Friendly wrapper layer
    // ═══════════════════════════════════════════════════════════
    //
    // SDL2-style: the raw @lib decls + struct types above are still
    // available for power users, but the classes / fns below let
    // callers skip their own `@extern(C) { ... }` block.

    // ─── Shader ────────────────────────────────────────────────

    /// Compiled WGSL shader module. Wraps a `WGPUShaderModule`
    /// handle. `Device.createWgslShader(code)` returns one;
    /// `release()` frees the underlying module.
    pub class Shader {
        pub handle: i64

        pub init(handle: WGPUShaderModule) {
            this.handle = handle as i64
        }
        pub release() {
            if this.handle != 0 {
                wgpuShaderModuleRelease(this.handle as WGPUShaderModule)
                this.handle = 0
            }
        }
    }

    // ─── Pipeline ──────────────────────────────────────────────

    /// Render pipeline. Built via `Device.createRenderPipeline`.
    pub class Pipeline {
        pub handle: i64

        pub init(handle: WGPURenderPipeline) {
            this.handle = handle as i64
        }
        pub release() {
            if this.handle != 0 {
                wgpuRenderPipelineRelease(this.handle as WGPURenderPipeline)
                this.handle = 0
            }
        }
    }

    /// Configuration for `Device.createRenderPipeline`. Defaults
    /// cover the "draw a single coloured triangle list against the
    /// surface format" path; tweak fields before passing in.
    pub class RenderPipelineOptions {
        pub vertexShader:   Shader
        pub vertexEntry:    string
        pub fragmentShader: Shader
        pub fragmentEntry:  string
        /// `TextureFormat` of the colour target. Use the value
        /// `GpuView.format` reports so the pipeline matches the
        /// surface.
        pub colorFormat:    TextureFormat
        pub topology:       u32     // PrimitiveTopology
        pub cullMode:       u32     // CullMode
        pub frontFace:      u32     // FrontFace

        /// Same shader module for vertex + fragment is the common
        /// case (one WGSL file with `vs_main` + `fs_main`).
        pub init(vertex: Shader, fragment: Shader, colorFormat: TextureFormat) {
            this.vertexShader   = vertex
            this.vertexEntry    = "vs_main"
            this.fragmentShader = fragment
            this.fragmentEntry  = "fs_main"
            this.colorFormat    = colorFormat
            this.topology       = 4 as u32    // TriangleList
            this.cullMode       = 1 as u32    // None
            this.frontFace      = 1 as u32    // CCW
        }
    }

    // ─── Device ────────────────────────────────────────────────

    // ─── Surface ───────────────────────────────────────────────

    /// Friendly wrapper around `WGPUSurface`. Owned by the
    /// gui-side surface host (`GpuView`); not released by this
    /// wrapper. User code rarely touches it directly — the
    /// surface lifecycle (configure / acquire / present) is
    /// driven by gui-core's render loop.
    pub class Surface {
        pub handle: i64

        pub init(handle: WGPUSurface) {
            this.handle = handle as i64
        }
    }

    // ─── Queue ─────────────────────────────────────────────────

    /// Friendly wrapper around `WGPUQueue`. The queue is owned by
    /// the underlying `WGPUDevice` (one queue per device); the
    /// gui-side surface host hands one out via `view.queue`. Not
    /// released by this wrapper.
    pub class Queue {
        pub handle: i64

        pub init(handle: WGPUQueue) {
            this.handle = handle as i64
        }
    }

    // ─── Device ────────────────────────────────────────────────

    /// Friendly wrapper around `WGPUDevice`. Construct from the
    /// device a `GpuView` (or other surface owner) exposes:
    ///   `let d = new wgpu.Device(view.device)`
    ///
    /// The underlying handle is NOT released on deinit — the
    /// gui-side surface owner owns it across the program.
    pub class Device {
        pub handle: i64

        pub init(handle: WGPUDevice) {
            this.handle = handle as i64
        }

        /// Compile a WGSL source string into a shader module.
        pub createWgslShader(wgsl: string): Shader {
            let src = WGPUShaderSourceWGSL {
                chain: WGPUChainedStruct { next: 0 as *void, sType: 2 as u32 },
                code:  WGPUStringView {
                    data:   cstrFromString(wgsl) as *void,
                    length: 0xFFFFFFFFFFFFFFFF as u64,    // WGPU_STRLEN
                },
            }
            let desc = WGPUShaderModuleDescriptor {
                nextInChain: (&src) as *void,
                label:       WGPUStringView { data: 0 as *void, length: 0 as u64 },
            }
            new Shader(wgpuDeviceCreateShaderModule(this.handle as WGPUDevice, &desc))
        }

        /// Build a render pipeline from the supplied options. No
        /// blend / depth-stencil / MSAA in this overload — the
        /// pipeline runs against `opts.colorFormat` with a single
        /// opaque write-mask = All and no depth attachment.
        pub createRenderPipeline(opts: RenderPipelineOptions): Pipeline {
            let target = WGPUColorTargetState {
                nextInChain: 0 as *void,
                format:      opts.colorFormat as u32,
                blend:       0 as *void,
                writeMask:   0xF as u64,
            }
            let fragState = WGPUFragmentState {
                nextInChain:   0 as *void,
                module:        opts.fragmentShader.handle as WGPUShaderModule,
                entryPoint:    WGPUStringView {
                    data:   cstrFromString(opts.fragmentEntry) as *void,
                    length: 0xFFFFFFFFFFFFFFFF as u64,
                },
                constantCount: 0 as u64,
                constants:     0 as *void,
                targetCount:   1 as u64,
                targets:       &target,
            }
            let desc = WGPURenderPipelineDescriptor {
                nextInChain: 0 as *void,
                label:       WGPUStringView { data: 0 as *void, length: 0 as u64 },
                layout:      0 as *void,
                vertex: WGPUVertexState {
                    nextInChain:   0 as *void,
                    module:        opts.vertexShader.handle as WGPUShaderModule,
                    entryPoint:    WGPUStringView {
                        data:   cstrFromString(opts.vertexEntry) as *void,
                        length: 0xFFFFFFFFFFFFFFFF as u64,
                    },
                    constantCount: 0 as u64,
                    constants:     0 as *void,
                    bufferCount:   0 as u64,
                    buffers:       0 as *void,
                },
                primitive: WGPUPrimitiveState {
                    nextInChain:      0 as *void,
                    topology:         opts.topology,
                    stripIndexFormat: 0 as u32,
                    frontFace:        opts.frontFace,
                    cullMode:         opts.cullMode,
                    unclippedDepth:   0 as u32,
                },
                depthStencil: 0 as *void,
                multisample: WGPUMultisampleState {
                    nextInChain:            0 as *void,
                    count:                  1 as u32,
                    mask:                   0xFFFFFFFF as u32,
                    alphaToCoverageEnabled: 0 as u32,
                },
                fragment: &fragState,
            }
            new Pipeline(wgpuDeviceCreateRenderPipeline(this.handle as WGPUDevice, &desc))
        }
    }

    // ─── RenderPass ────────────────────────────────────────────

    /// Friendly wrapper around a `WGPURenderPassEncoder` that the
    /// gui-side `onRender` callback hands to user code. The
    /// underlying encoder's lifecycle (begin / end / submit /
    /// present) is owned by gui-core; this wrapper exposes only
    /// the recording verbs.
    pub class RenderPass {
        pub handle: i64

        pub init(handle: WGPURenderPassEncoder) {
            this.handle = handle as i64
        }

        pub setPipeline(pipeline: Pipeline) {
            wgpuRenderPassEncoderSetPipeline(
                this.handle as WGPURenderPassEncoder,
                pipeline.handle as WGPURenderPipeline
            )
        }

        /// Record a draw of `vertexCount` vertices into
        /// `instanceCount` instances, starting at
        /// `firstVertex` / `firstInstance`. For the hello-triangle
        /// case use `draw(3, 1, 0, 0)`.
        ///
        /// Counts are i64 here so call sites don't need `as u32`
        /// casts on integer literals; the wgpu C ABI gets the
        /// expected u32 inside.
        pub draw(
            vertexCount: i64, instanceCount: i64,
            firstVertex: i64, firstInstance: i64
        ) {
            wgpuRenderPassEncoderDraw(
                this.handle as WGPURenderPassEncoder,
                vertexCount as u32, instanceCount as u32,
                firstVertex as u32, firstInstance as u32
            )
        }
    }
}
