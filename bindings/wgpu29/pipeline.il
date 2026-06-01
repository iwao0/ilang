/// Render pipelines + compute pipelines + every state struct
/// (`VertexState`, `PrimitiveState`, `FragmentState`,
/// `ColorTargetState`, `MultisampleState`, `VertexAttribute`,
/// `VertexBufferLayout`, …). Friendly `Pipeline` /
/// `RenderPipelineOptions` wrappers live here too. The `Device`
/// wrapper lives in `device.il`.

use core { WGPUStringView, WGPUDevice, TextureFormat }
use shader { WGPUShaderModule, Shader }
use bind_group { WGPUBindGroupLayout }

pub enum BlendFactor: u32 {
    undefined          = 0x00
    zero               = 0x01
    one                = 0x02
    src                = 0x03
    oneMinusSrc        = 0x04
    srcAlpha           = 0x05
    oneMinusSrcAlpha   = 0x06
    dst                = 0x07
    oneMinusDst        = 0x08
    dstAlpha           = 0x09
    oneMinusDstAlpha   = 0x0A
    srcAlphaSaturated  = 0x0B
    constant           = 0x0C
    oneMinusConstant   = 0x0D
    src1               = 0x0E
    oneMinusSrc1       = 0x0F
    src1Alpha          = 0x10
    oneMinusSrc1Alpha  = 0x11
}

pub enum BlendOperation: u32 {
    undefined       = 0x00
    add             = 0x01
    subtract        = 0x02
    reverseSubtract = 0x03
    min             = 0x04
    max             = 0x05
}

pub enum CullMode: u32 {
    undefined = 0x00
    none      = 0x01
    front     = 0x02
    back      = 0x03
}

pub enum FrontFace: u32 {
    undefined = 0x00
    ccw       = 0x01
    cw        = 0x02
}

pub enum IndexFormat: u32 {
    undefined = 0x00
    uint16    = 0x01
    uint32    = 0x02
}

pub enum PrimitiveTopology: u32 {
    undefined     = 0x00
    pointList     = 0x01
    lineList      = 0x02
    lineStrip     = 0x03
    triangleList  = 0x04
    triangleStrip = 0x05
}

pub enum StencilOperation: u32 {
    undefined      = 0x00
    keep           = 0x01
    zero           = 0x02
    replace        = 0x03
    invert         = 0x04
    incrementClamp = 0x05
    decrementClamp = 0x06
    incrementWrap  = 0x07
    decrementWrap  = 0x08
}

pub enum VertexFormat: u32 {
    uint8           = 0x01
    uint8x2         = 0x02
    uint8x4         = 0x03
    sint8           = 0x04
    sint8x2         = 0x05
    sint8x4         = 0x06
    unorm8          = 0x07
    unorm8x2        = 0x08
    unorm8x4        = 0x09
    snorm8          = 0x0A
    snorm8x2        = 0x0B
    snorm8x4        = 0x0C
    uint16          = 0x0D
    uint16x2        = 0x0E
    uint16x4        = 0x0F
    sint16          = 0x10
    sint16x2        = 0x11
    sint16x4        = 0x12
    unorm16         = 0x13
    unorm16x2       = 0x14
    unorm16x4       = 0x15
    snorm16         = 0x16
    snorm16x2       = 0x17
    snorm16x4       = 0x18
    float16         = 0x19
    float16x2       = 0x1A
    float16x4       = 0x1B
    float32         = 0x1C
    float32x2       = 0x1D
    float32x3       = 0x1E
    float32x4       = 0x1F
    uint32          = 0x20
    uint32x2        = 0x21
    uint32x3        = 0x22
    uint32x4        = 0x23
    sint32          = 0x24
    sint32x2        = 0x25
    sint32x3        = 0x26
    sint32x4        = 0x27
    unorm10_10_10_2 = 0x28
    unorm8x4BGRA    = 0x29
}

pub enum VertexStepMode: u32 {
    undefined = 0x00
    vertex    = 0x01
    instance  = 0x02
}

pub enum PolygonMode: u32 {
    fill  = 0x00
    line  = 0x01
    point = 0x02
}

@flags
pub enum ColorWriteMask: u64 {
    none  = 0x00
    red   = 0x01
    green = 0x02
    blue  = 0x04
    alpha = 0x08
    all   = 0x0F
}

@extern(C, "wgpu_native") {
    @handle pub struct WGPURenderPipeline {}
    @handle pub struct WGPUComputePipeline {}

    // ─── render pipeline state structs ────────────────────────
    pub struct WGPUColorTargetState {
        nextInChain: *void
        format:      TextureFormat
        blend:       *void    // WGPUBlendState* (nullable)
        writeMask:   ColorWriteMask
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
        topology:         PrimitiveTopology
        stripIndexFormat: IndexFormat
        frontFace:        FrontFace
        cullMode:         CullMode
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
        format:         VertexFormat
        offset:         u64
        shaderLocation: u32
    }
    pub struct WGPUVertexBufferLayout {
        nextInChain:    *void
        stepMode:       VertexStepMode
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
        mode:        CallbackMode
        callback:    fn(i32, i64, i64, i64, i64, i64)
        userdata1:   *void
        userdata2:   *void
    }
    pub struct WGPUCreateComputePipelineAsyncCallbackInfo {
        nextInChain: *void
        mode:        CallbackMode
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
        pub topology:       PrimitiveTopology
        pub cullMode:       CullMode
        pub frontFace:      FrontFace

        /// Same shader module for vertex + fragment is the
        /// common case (one WGSL file with `vs_main` + `fs_main`).
        pub init(vertex: Shader, fragment: Shader, colorFormat: TextureFormat) {
            this.vertexShader   = vertex
            this.vertexEntry    = "vs_main"
            this.fragmentShader = fragment
            this.fragmentEntry  = "fs_main"
            this.colorFormat    = colorFormat
            this.topology       = PrimitiveTopology.triangleList
            this.cullMode       = CullMode.none
            this.frontFace      = FrontFace.ccw
        }
    }

}
