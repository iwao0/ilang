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
    // NOTE: handle-typed fields are declared `*void` (not the
    // `@handle` type) because ilang currently mis-serialises a
    // `@handle`-typed field inside an `@extern(C)` struct — the
    // stored value doesn't round-trip. Handles pass fine as direct
    // function arguments, so only struct *fields* use `*void` +
    // an `as` cast at the call site.
    pub struct WGPURequestAdapterOptions {
        nextInChain:          *void
        featureLevel:         u32
        powerPreference:      u32
        forceFallbackAdapter: u32     // WGPUBool
        backendType:          u32
        compatibleSurface:    *void    // WGPUSurface
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
    pub struct WGPUSurfaceDescriptor {
        nextInChain: *void        // WGPUChainedStruct*
        label:       WGPUStringView
    }
    // format = WGPUTextureFormat (enum, 4B). usage = WGPUTextureUsage
    // (WGPUFlags = uint64). RenderAttachment = 0x10.
    pub struct WGPUSurfaceConfiguration {
        nextInChain:     *void
        device:          *void        // WGPUDevice
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
        texture:     *void        // WGPUTexture
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
        view:          *void      // WGPUTextureView
        depthSlice:    u32        // WGPU_DEPTH_SLICE_UNDEFINED = 0xFFFFFFFF
        resolveTarget: *void      // WGPUTextureView (nullable)
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
    // `commands` is a `WGPUCommandBuffer const*` (array). Pass an
    // `i64[]` of command-buffer handles (i64[] → *i64 coercion) to
    // avoid taking `&` of a `@handle` local.
    @lib pub fn wgpuQueueSubmit(
        queue: WGPUQueue, commandCount: u64, commands: *i64
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
        module:        *void  // WGPUShaderModule
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
        module:        *void  // WGPUShaderModule
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

    // ─── release (avoid leaks across frames) ──────────────────
    @lib pub fn wgpuTextureViewRelease(view: WGPUTextureView)
    @lib pub fn wgpuTextureRelease(texture: WGPUTexture)
    @lib pub fn wgpuRenderPassEncoderRelease(encoder: WGPURenderPassEncoder)
    @lib pub fn wgpuCommandEncoderRelease(encoder: WGPUCommandEncoder)
    @lib pub fn wgpuCommandBufferRelease(commandBuffer: WGPUCommandBuffer)
    @lib pub fn wgpuShaderModuleRelease(shaderModule: WGPUShaderModule)
    @lib pub fn wgpuRenderPipelineRelease(pipeline: WGPURenderPipeline)
}
