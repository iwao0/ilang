/// Render pipelines + compute pipelines + every state struct
/// (`VertexState`, `PrimitiveState`, `FragmentState`,
/// `ColorTargetState`, `MultisampleState`, `VertexAttribute`,
/// `VertexBufferLayout`, …). Friendly `Pipeline` /
/// `RenderPipelineOptions` wrappers live here too. The `Device`
/// wrapper lives in `device.il`.

use core { WGPUStringView, WGPUDevice, TextureFormat }
use shader { WGPUShaderModule, Shader }
use bind_group { WGPUBindGroupLayout }

@extern(C, "wgpu_native") {
    @handle pub struct WGPURenderPipeline {}
    @handle pub struct WGPUComputePipeline {}

    // ─── render pipeline state structs ────────────────────────
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

    @lib pub fn wgpuDeviceCreateRenderPipeline(
        device: WGPUDevice, descriptor: *WGPURenderPipelineDescriptor
    ): WGPURenderPipeline
    // Pull a single bind-group layout out of an auto-layout
    // pipeline (one created with `layout = NULL`). `groupIndex`
    // matches the `@group(N)` annotation in WGSL.
    @lib pub fn wgpuRenderPipelineGetBindGroupLayout(
        pipeline: WGPURenderPipeline, groupIndex: u32
    ): WGPUBindGroupLayout
    @lib pub fn wgpuRenderPipelineSetLabel(pipeline: WGPURenderPipeline, label: WGPUStringView)
    @lib pub fn wgpuRenderPipelineAddRef(pipeline: WGPURenderPipeline)
    @lib pub fn wgpuRenderPipelineRelease(pipeline: WGPURenderPipeline)

    // ─── compute pipeline ─────────────────────────────────────
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
        layout:      *void            // WGPUPipelineLayout (nullable)
        compute:     WGPUComputeState
    }
    @lib pub fn wgpuDeviceCreateComputePipeline(
        device: WGPUDevice, descriptor: *WGPUComputePipelineDescriptor
    ): WGPUComputePipeline
    @lib pub fn wgpuComputePipelineGetBindGroupLayout(
        pipeline: WGPUComputePipeline, groupIndex: u32
    ): WGPUBindGroupLayout
    @lib pub fn wgpuComputePipelineSetLabel(pipeline: WGPUComputePipeline, label: WGPUStringView)
    @lib pub fn wgpuComputePipelineAddRef(pipeline: WGPUComputePipeline)
    @lib pub fn wgpuComputePipelineRelease(pipeline: WGPUComputePipeline)

    // ─── async pipeline creation ──────────────────────────────
    //
    // Async variants return a `WGPUFuture` (u64 id) immediately
    // and resolve the supplied callback when the driver finishes
    // compiling. Callback shape:
    //   void(WGPUCreatePipelineAsyncStatus status,
    //        WGPU{Render,Compute}Pipeline pipeline,
    //        WGPUStringView message, void* ud1, void* ud2)
    // → (status: i32, pipeline: i64, msg.data: i64, msg.len: i64,
    //    ud1: i64, ud2: i64).
    pub struct WGPUCreateRenderPipelineAsyncCallbackInfo {
        nextInChain: *void
        mode:        u32                            // WGPUCallbackMode
        callback:    fn(i32, i64, i64, i64, i64, i64)
        userdata1:   *void
        userdata2:   *void
    }
    pub struct WGPUCreateComputePipelineAsyncCallbackInfo {
        nextInChain: *void
        mode:        u32                            // WGPUCallbackMode
        callback:    fn(i32, i64, i64, i64, i64, i64)
        userdata1:   *void
        userdata2:   *void
    }
    @lib pub fn wgpuDeviceCreateRenderPipelineAsync(
        device:       WGPUDevice,
        descriptor:   *WGPURenderPipelineDescriptor,
        callbackInfo: WGPUCreateRenderPipelineAsyncCallbackInfo
    ): u64                                          // WGPUFuture
    @lib pub fn wgpuDeviceCreateComputePipelineAsync(
        device:       WGPUDevice,
        descriptor:   *WGPUComputePipelineDescriptor,
        callbackInfo: WGPUCreateComputePipelineAsyncCallbackInfo
    ): u64                                          // WGPUFuture

    // ═══════════════════════════════════════════════════════════
    //  Friendly wrapper layer
    // ═══════════════════════════════════════════════════════════

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
    /// cover the "draw a single coloured triangle list against
    /// the surface format" path; tweak fields before passing in.
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

        /// Same shader module for vertex + fragment is the
        /// common case (one WGSL file with `vs_main` + `fs_main`).
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

}
