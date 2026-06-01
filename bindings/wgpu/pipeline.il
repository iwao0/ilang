/// Render pipelines + compute pipelines + every state struct
/// (`VertexState`, `PrimitiveState`, `FragmentState`,
/// `ColorTargetState`, `MultisampleState`, `VertexAttribute`,
/// `VertexBufferLayout`, …). Friendly `Pipeline` /
/// `RenderPipelineOptions` classes live here too.
///
/// The `Device` friendly class also lives here because its
/// `createWgslShader` / `createRenderPipeline` methods need to
/// return `Shader` / `Pipeline` instances declared above —
/// putting it in `device.il` would create a forward dependency
/// the other way.

use core { WGPUStringView, WGPUChainedStruct, TextureFormat }
use device { WGPUDevice }
use shader { WGPUShaderModule, WGPUShaderSourceWGSL, WGPUShaderModuleDescriptor, Shader, wgpuDeviceCreateShaderModule }
use bind_group { WGPUBindGroupLayout }
use std.ffi { cstrFromString }

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
    @lib pub fn wgpuComputePipelineRelease(pipeline: WGPUComputePipeline)

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
}
