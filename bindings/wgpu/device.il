/// Adapter→device promotion + queue retrieval + the wgpu-native
/// `DevicePoll` extension + the friendly `Device` class that
/// turns a raw `WGPUDevice` into a method-chained API.
///
/// `WGPUDevice` and `WGPUQueue` handles live in `core.il` (so
/// downstream feature files can declare `wgpuDeviceCreate*(...)`
/// without cycling back through this file's Device class).

use core { WGPUDevice, WGPUQueue, WGPUStringView, WGPUChainedStruct, TextureFormat }
use adapter { WGPUAdapter }
use shader {
    Shader, WGPUShaderModule, WGPUShaderSourceWGSL,
    WGPUShaderModuleDescriptor, wgpuDeviceCreateShaderModule,
}
use pipeline {
    Pipeline, RenderPipelineOptions,
    WGPURenderPipelineDescriptor,
    WGPUVertexState, WGPUFragmentState, WGPUColorTargetState,
    WGPUPrimitiveState, WGPUMultisampleState,
    wgpuDeviceCreateRenderPipeline,
}
use std.ffi { cstrFromString }

@extern(C, "wgpu_native") {
    // Same layout as RequestAdapterCallbackInfo; callback is
    //   void(WGPURequestDeviceStatus, WGPUDevice, WGPUStringView,
    //        void*, void*)
    pub struct WGPURequestDeviceCallbackInfo {
        nextInChain: *void
        mode:        u32
        callback:    fn(i32, i64, i64, i64, i64, i64)
        userdata1:   *void
        userdata2:   *void
    }

    @lib pub fn wgpuAdapterRequestDevice(
        adapter:      WGPUAdapter,
        descriptor:   *void,
        callbackInfo: WGPURequestDeviceCallbackInfo
    ): u64
    @lib pub fn wgpuDeviceGetQueue(device: WGPUDevice): WGPUQueue

    // wgpu-native extension. `wait` = 1 blocks until pending work
    // and any registered map callbacks complete; pass null for
    // the submission-index pointer to poll on every queued
    // submit.
    @lib pub fn wgpuDevicePoll(
        device: WGPUDevice, wait: u32, submissionIndex: *const u64
    ): u32

    // ─── friendly wrapper ─────────────────────────────────────

    /// Wraps `WGPUDevice`. Construct from the device a `GpuView`
    /// (or other surface owner) exposes:
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
