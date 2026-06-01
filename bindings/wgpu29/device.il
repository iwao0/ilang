/// Adapter→device promotion + queue retrieval + the wgpu-native
/// `DevicePoll` extension + the friendly `Device` class that
/// turns a raw `WGPUDevice` into a method-chained API.
///
/// `WGPUDevice` and `WGPUQueue` handles live in `core.il` (so
/// downstream feature files can declare `wgpuDeviceCreate*(...)`
/// without cycling back through this file's Device class).

use core {
    WGPUDevice, WGPUQueue, WGPUStringView, WGPUChainedStruct,
    WGPULimits, TextureFormat, SType,
    WGPUAdapter, WGPUAdapterInfo, WGPUSupportedFeatures,
    BackendType, AdapterType,
    wgpuAdapterInfoFreeMembers,
}
use shader {
    Shader, WGPUShaderModule, WGPUShaderSourceWGSL,
    WGPUShaderModuleDescriptor, wgpuDeviceCreateShaderModule,
}
use pipeline {
    Pipeline, RenderPipelineOptions,
    WGPURenderPipelineDescriptor,
    WGPUVertexState, WGPUFragmentState, WGPUColorTargetState,
    WGPUPrimitiveState, WGPUMultisampleState,
    ColorWriteMask, IndexFormat,
    ComputePipeline, ComputePipelineOptions,
    WGPUComputePipeline, WGPUComputePipelineDescriptor, WGPUComputeState,
    wgpuDeviceCreateRenderPipeline, wgpuDeviceCreateComputePipeline,
}
use buffer {
    Buffer, BufferOptions,
    WGPUBuffer, WGPUBufferDescriptor, BufferUsage, wgpuDeviceCreateBuffer,
}
use texture {
    Texture, TextureOptions, TextureViewDimension,
    WGPUTexture, WGPUTextureView, WGPUTextureDescriptor, wgpuDeviceCreateTexture,
}
use core { WGPUExtent3D }
use bind_group {
    Sampler, SamplerOptions,
    BindGroup, BindGroupLayout, PipelineLayout,
    BindGroupOptions, BindGroupEntryOption, BindingResourceKind,
    BindGroupLayoutOptions, BindGroupLayoutEntryOption, BindingLayoutKind,
    PipelineLayoutOptions,
    BufferBindingType, SamplerBindingType, TextureSampleType,
    StorageTextureAccess,
    WGPUBindGroup, WGPUBindGroupLayout, WGPUPipelineLayout, WGPUSampler,
    WGPUBindGroupDescriptor, WGPUBindGroupLayoutDescriptor,
    WGPUPipelineLayoutDescriptor, WGPUSamplerDescriptor,
    WGPUBindGroupEntry, WGPUBindGroupLayoutEntry,
    WGPUBufferBindingLayout, WGPUSamplerBindingLayout,
    WGPUTextureBindingLayout, WGPUStorageTextureBindingLayout,
    wgpuDeviceCreateBindGroup, wgpuDeviceCreateBindGroupLayout,
    wgpuDeviceCreatePipelineLayout, wgpuDeviceCreateSampler,
}
use command {
    CommandEncoder,
    WGPUCommandEncoder, wgpuDeviceCreateCommandEncoder,
}
use query {
    QuerySet, QuerySetOptions,
    WGPUQuerySet, WGPUQuerySetDescriptor, QueryType, wgpuDeviceCreateQuerySet,
}
use std.ffi { cstrFromString, stringFromCstr }

pub enum CompilationInfoRequestStatus: u32 {
    success           = 0x01
    callbackCancelled = 0x02
}

pub enum CompilationMessageType: u32 {
    error   = 0x01
    warning = 0x02
    info    = 0x03
}

pub enum CreatePipelineAsyncStatus: u32 {
    success           = 0x01
    callbackCancelled = 0x02
    validationError   = 0x03
    internalError     = 0x04
}

pub enum DeviceLostReason: u32 {
    unknown           = 0x01
    destroyed         = 0x02
    callbackCancelled = 0x03
    failedCreation    = 0x04
}

pub enum ErrorFilter: u32 {
    validation  = 0x01
    outOfMemory = 0x02
    internal    = 0x03
}

pub enum ErrorType: u32 {
    noError     = 0x01
    validation  = 0x02
    outOfMemory = 0x03
    internal    = 0x04
    unknown     = 0x05
}

pub enum PopErrorScopeStatus: u32 {
    success           = 0x01
    callbackCancelled = 0x02
    error             = 0x03
}

@extern(C, "wgpu_native") {
    // Same layout as RequestAdapterCallbackInfo; callback is
    //   void(WGPURequestDeviceStatus, WGPUDevice, WGPUStringView,
    //        void*, void*)
    pub struct WGPURequestDeviceCallbackInfo {
        nextInChain: *void
        mode:        CallbackMode
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

    // ─── introspection ────────────────────────────────────────
    @lib pub fn wgpuDeviceGetLimits(
        device: WGPUDevice, limits: *WGPULimits
    ): u32                                          // WGPUStatus
    @lib pub fn wgpuDeviceGetFeatures(
        device: WGPUDevice, features: *WGPUSupportedFeatures
    )
    @lib pub fn wgpuDeviceHasFeature(
        device: WGPUDevice, feature: u32            // WGPUFeatureName
    ): u32                                          // WGPUBool
    @lib pub fn wgpuDeviceGetAdapterInfo(
        device: WGPUDevice, adapterInfo: *WGPUAdapterInfo
    ): u32                                          // WGPUStatus
    // Returns a WGPUFuture (u64 id). The associated callback
    // fires via the lost-future awaiter; see the wgpu_native
    // examples for the WaitAny + ProcessEvents pattern.
    @lib pub fn wgpuDeviceGetLostFuture(device: WGPUDevice): u64

    // ─── error scopes ─────────────────────────────────────────
    //
    // Push at the start of a block of suspicious work, pop with
    // a callback that receives the first error (if any) that
    // matched `filter`. WGPUErrorFilter: Validation=1,
    // OutOfMemory=2, Internal=3. The pop callback receives
    //   (WGPUPopErrorScopeStatus status, WGPUErrorType type,
    //    WGPUStringView message, void* ud1, void* ud2)
    // with the message splayed to (data, len) i64s.
    pub struct WGPUPopErrorScopeCallbackInfo {
        nextInChain: *void
        mode:        CallbackMode
        callback:    fn(i32, i32, i64, i64, i64, i64)
        userdata1:   *void
        userdata2:   *void
    }
    @lib pub fn wgpuDevicePushErrorScope(
        device: WGPUDevice, filter: u32             // WGPUErrorFilter
    )
    @lib pub fn wgpuDevicePopErrorScope(
        device: WGPUDevice, callbackInfo: WGPUPopErrorScopeCallbackInfo
    ): u64                                          // WGPUFuture

    // Drop GPU resources eagerly without releasing the host-side
    // wrapper. Same caveat as wgpuBufferDestroy — any in-flight
    // command referencing the device will fail validation.
    @lib pub fn wgpuDeviceDestroy(device: WGPUDevice)
    @lib pub fn wgpuDeviceSetLabel(device: WGPUDevice, label: WGPUStringView)
    @lib pub fn wgpuDeviceAddRef(device: WGPUDevice)
    @lib pub fn wgpuDeviceRelease(device: WGPUDevice)

    // wgpu-native extension. Start / Stop the platform GPU
    // debugger capture (RenderDoc on Windows/Linux, Xcode-frame-
    // debugger on macOS) for diagnostic profiling. Start returns
    // false if no debugger is attached or the capture failed to
    // start; Stop has no return.
    @lib pub fn wgpuDeviceStartGraphicsDebuggerCapture(device: WGPUDevice): u32
    @lib pub fn wgpuDeviceStopGraphicsDebuggerCapture(device: WGPUDevice)

    // wgpu-native macOS extension. Returns the underlying Metal
    // `id<MTLDevice>` for interop with native Metal code (cast
    // the `*void` to NSObject* on the call site). No-op + null
    // on other backends.
    @lib pub fn wgpuDeviceGetNativeMetalDevice(device: WGPUDevice): *void

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
                chain: WGPUChainedStruct { next: 0 as *void, sType: SType.shaderSourceWGSL },
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
                format:      opts.colorFormat,
                blend:       0 as *void,
                writeMask:   ColorWriteMask.all,
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
                    stripIndexFormat: IndexFormat.undefined,
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

        // ─── plain resource factories ────────────────────────
        //
        // ilang friendly-class methods can't take parameters
        // whose type transitively exposes `*void` (every
        // `WGPUFooDescriptor.nextInChain` triggers it). For
        // resources with array-bearing descriptors (BindGroup,
        // BindGroupLayout, PipelineLayout) callers build the
        // struct themselves and invoke the raw
        // `wgpuDeviceCreate*` from the top-level umbrella.

        /// Default-config sampler (ClampToEdge / Nearest / no
        /// compare) — same as `wgpuDeviceCreateSampler(d, null)`.
        pub createDefaultSampler(): Sampler {
            new Sampler(wgpuDeviceCreateSampler(
                this.handle as WGPUDevice, 0 as *WGPUSamplerDescriptor
            ))
        }
        /// Build a sampler from the supplied options.
        pub createSampler(opts: SamplerOptions): Sampler {
            let desc = WGPUSamplerDescriptor {
                nextInChain:   0 as *void,
                label:         WGPUStringView { data: 0 as *void, length: 0 as u64 },
                addressModeU:  opts.addressModeU,
                addressModeV:  opts.addressModeV,
                addressModeW:  opts.addressModeW,
                magFilter:     opts.magFilter,
                minFilter:     opts.minFilter,
                mipmapFilter:  opts.mipmapFilter,
                lodMinClamp:   opts.lodMinClamp,
                lodMaxClamp:   opts.lodMaxClamp,
                compare:       opts.compare,
                maxAnisotropy: opts.maxAnisotropy as u16,
            }
            new Sampler(wgpuDeviceCreateSampler(this.handle as WGPUDevice, &desc))
        }
        /// Build a `BindGroup` from `opts`. Walks
        /// `opts.entries`, fills a parallel raw
        /// `WGPUBindGroupEntry[]` with the correct handle slot
        /// per `kind` (others zero), then calls
        /// `wgpuDeviceCreateBindGroup`.
        pub createBindGroup(opts: BindGroupOptions): BindGroup {
            let raw: WGPUBindGroupEntry[] = []
            let i: i64 = 0
            while i < opts.entries.length {
                let e = opts.entries[i]
                let bufHandle:     WGPUBuffer      = 0 as WGPUBuffer
                let bufOffset:     u64             = 0 as u64
                let bufSize:       u64             = 0 as u64
                let samHandle:     WGPUSampler     = 0 as WGPUSampler
                let viewHandle:    WGPUTextureView = 0 as WGPUTextureView
                if e.kind == BindingResourceKind.buffer {
                    bufHandle = e.buffer.handle as WGPUBuffer
                    bufOffset = e.bufferOffset
                    bufSize   = e.bufferSize
                    if bufSize == (0 as u64) {
                        bufSize = 0xFFFFFFFFFFFFFFFF as u64
                    }
                }
                if e.kind == BindingResourceKind.sampler {
                    samHandle = e.sampler.handle as WGPUSampler
                }
                if e.kind == BindingResourceKind.textureView {
                    viewHandle = e.textureView.handle as WGPUTextureView
                }
                raw.push(WGPUBindGroupEntry {
                    nextInChain: 0 as *void,
                    binding:     e.binding as u32,
                    buffer:      bufHandle,
                    offset:      bufOffset,
                    size:        bufSize,
                    sampler:     samHandle,
                    textureView: viewHandle,
                })
                i = i + 1
            }
            let desc = WGPUBindGroupDescriptor {
                nextInChain: 0 as *void,
                label:       WGPUStringView {
                    data:   cstrFromString(opts.label) as *void,
                    length: 0xFFFFFFFFFFFFFFFF as u64,
                },
                layout:      opts.layout.handle as WGPUBindGroupLayout,
                entryCount:  raw.length as u64,
                entries:     raw,
            }
            new BindGroup(wgpuDeviceCreateBindGroup(
                this.handle as WGPUDevice, &desc
            ))
        }

        /// Build a `BindGroupLayout` from `opts`. Each
        /// `BindGroupLayoutEntryOption` selects exactly one of
        /// the four nested binding-layout sub-structs via
        /// `kind`; the others are zero-initialised.
        pub createBindGroupLayout(opts: BindGroupLayoutOptions): BindGroupLayout {
            let raw: WGPUBindGroupLayoutEntry[] = []
            let i: i64 = 0
            while i < opts.entries.length {
                let e = opts.entries[i]

                let bufType:     BufferBindingType    = BufferBindingType.bindingNotUsed
                let bufDyn:      u32                  = 0 as u32
                let bufMin:      u64                  = 0 as u64

                let samType:     SamplerBindingType   = SamplerBindingType.bindingNotUsed

                let texSample:   TextureSampleType    = TextureSampleType.bindingNotUsed
                let texViewDim:  TextureViewDimension = TextureViewDimension.undefined
                let texMulti:    u32                  = 0 as u32

                let stAccess:    StorageTextureAccess = StorageTextureAccess.bindingNotUsed
                let stFormat:    TextureFormat        = TextureFormat.undefined
                let stViewDim:   TextureViewDimension = TextureViewDimension.undefined

                if e.kind == BindingLayoutKind.buffer {
                    bufType = e.bufferType
                    if e.bufferHasDynamicOffset { bufDyn = 1 as u32 }
                    bufMin  = e.bufferMinBindingSize
                }
                if e.kind == BindingLayoutKind.sampler {
                    samType = e.samplerType
                }
                if e.kind == BindingLayoutKind.texture {
                    texSample  = e.textureSampleType
                    texViewDim = e.textureViewDimension
                    if e.textureMultisampled { texMulti = 1 as u32 }
                }
                if e.kind == BindingLayoutKind.storageTexture {
                    stAccess  = e.storageAccess
                    stFormat  = e.storageFormat
                    stViewDim = e.storageViewDimension
                }

                raw.push(WGPUBindGroupLayoutEntry {
                    nextInChain:      0 as *void,
                    binding:          e.binding as u32,
                    visibility:       e.visibility,
                    bindingArraySize: 0 as u32,
                    buffer:           WGPUBufferBindingLayout {
                        nextInChain:      0 as *void,
                        type:             bufType,
                        hasDynamicOffset: bufDyn,
                        minBindingSize:   bufMin,
                    },
                    sampler:          WGPUSamplerBindingLayout {
                        nextInChain: 0 as *void,
                        type:        samType,
                    },
                    texture:          WGPUTextureBindingLayout {
                        nextInChain:   0 as *void,
                        sampleType:    texSample,
                        viewDimension: texViewDim,
                        multisampled:  texMulti,
                    },
                    storageTexture:   WGPUStorageTextureBindingLayout {
                        nextInChain:   0 as *void,
                        access:        stAccess,
                        format:        stFormat,
                        viewDimension: stViewDim,
                    },
                })
                i = i + 1
            }
            let desc = WGPUBindGroupLayoutDescriptor {
                nextInChain: 0 as *void,
                label:       WGPUStringView {
                    data:   cstrFromString(opts.label) as *void,
                    length: 0xFFFFFFFFFFFFFFFF as u64,
                },
                entryCount:  raw.length as u64,
                entries:     raw,
            }
            new BindGroupLayout(wgpuDeviceCreateBindGroupLayout(
                this.handle as WGPUDevice, &desc
            ))
        }

        /// Build a `PipelineLayout` from a list of bind-group
        /// layouts. The resulting layout binds each
        /// `opts.layouts[i]` to `@group(i)` in the WGSL.
        pub createPipelineLayout(opts: PipelineLayoutOptions): PipelineLayout {
            let raw: WGPUBindGroupLayout[] = []
            let i: i64 = 0
            while i < opts.layouts.length {
                raw.push(opts.layouts[i].handle as WGPUBindGroupLayout)
                i = i + 1
            }
            let desc = WGPUPipelineLayoutDescriptor {
                nextInChain:          0 as *void,
                label:                WGPUStringView {
                    data:   cstrFromString(opts.label) as *void,
                    length: 0xFFFFFFFFFFFFFFFF as u64,
                },
                bindGroupLayoutCount: raw.length as u64,
                bindGroupLayouts:     raw,
            }
            new PipelineLayout(wgpuDeviceCreatePipelineLayout(
                this.handle as WGPUDevice, &desc
            ))
        }

        /// `WGPUCommandEncoderDescriptor` only carries an
        /// optional label; this no-arg overload skips it.
        pub createCommandEncoder(): CommandEncoder {
            new CommandEncoder(wgpuDeviceCreateCommandEncoder(
                this.handle as WGPUDevice, 0 as *void
            ))
        }
        /// Simple-buffer convenience — no label, no
        /// mapped-at-creation. Use the `BufferOptions` overload
        /// for labelled / mapped buffers.
        pub createBuffer(usage: BufferUsage, size: u64): Buffer {
            let desc = WGPUBufferDescriptor {
                nextInChain:      0 as *void,
                label:            WGPUStringView { data: 0 as *void, length: 0 as u64 },
                usage:            usage,
                size:             size,
                mappedAtCreation: 0 as u32,
            }
            new Buffer(wgpuDeviceCreateBuffer(this.handle as WGPUDevice, &desc))
        }
        /// Full-control buffer creation — label + mapped-at-creation.
        pub createBufferWithOptions(opts: BufferOptions): Buffer {
            let mapped: u32 = 0
            if opts.mappedAtCreation { mapped = 1 }
            let desc = WGPUBufferDescriptor {
                nextInChain:      0 as *void,
                label:            WGPUStringView { data: 0 as *void, length: 0 as u64 },
                usage:            opts.usage,
                size:             opts.size,
                mappedAtCreation: mapped,
            }
            new Buffer(wgpuDeviceCreateBuffer(this.handle as WGPUDevice, &desc))
        }
        /// 2D / 3D texture creation.
        pub createTexture(opts: TextureOptions): Texture {
            let desc = WGPUTextureDescriptor {
                nextInChain:     0 as *void,
                label:           WGPUStringView { data: 0 as *void, length: 0 as u64 },
                usage:           opts.usage,
                dimension:       opts.dimension,
                size:            WGPUExtent3D {
                    width:              opts.width as u32,
                    height:             opts.height as u32,
                    depthOrArrayLayers: opts.depthOrArrayLayers as u32,
                },
                format:          opts.format,
                mipLevelCount:   opts.mipLevelCount as u32,
                sampleCount:     opts.sampleCount as u32,
                viewFormatCount: 0 as u64,
                viewFormats:     0 as *void,
            }
            new Texture(wgpuDeviceCreateTexture(this.handle as WGPUDevice, &desc))
        }
        /// GPU-side counter buffer (occlusion / timestamp).
        pub createQuerySet(opts: QuerySetOptions): QuerySet {
            let desc = WGPUQuerySetDescriptor {
                nextInChain: 0 as *void,
                label:       WGPUStringView { data: 0 as *void, length: 0 as u64 },
                type:        opts.type,
                count:       opts.count as u32,
            }
            new QuerySet(wgpuDeviceCreateQuerySet(this.handle as WGPUDevice, &desc))
        }
        /// Compute pipeline — auto-derived pipeline layout.
        pub createComputePipeline(opts: ComputePipelineOptions): ComputePipeline {
            let desc = WGPUComputePipelineDescriptor {
                nextInChain: 0 as *void,
                label:       WGPUStringView { data: 0 as *void, length: 0 as u64 },
                layout:      0 as *void,
                compute: WGPUComputeState {
                    nextInChain:   0 as *void,
                    module:        opts.shader.handle as WGPUShaderModule,
                    entryPoint:    WGPUStringView {
                        data:   cstrFromString(opts.entryPoint) as *void,
                        length: 0xFFFFFFFFFFFFFFFF as u64,
                    },
                    constantCount: 0 as u64,
                    constants:     0 as *void,
                },
            }
            new ComputePipeline(wgpuDeviceCreateComputePipeline(
                this.handle as WGPUDevice, &desc
            ))
        }

        /// Capability limits for this device. Value-copy of
        /// `WGPULimits`; safe to keep across the device's
        /// lifetime.
        pub getLimits(): Limits {
            let raw = WGPULimits {
                nextInChain:                                  0 as *void,
                maxTextureDimension1D:                        0 as u32,
                maxTextureDimension2D:                        0 as u32,
                maxTextureDimension3D:                        0 as u32,
                maxTextureArrayLayers:                        0 as u32,
                maxBindGroups:                                0 as u32,
                maxBindGroupsPlusVertexBuffers:               0 as u32,
                maxBindingsPerBindGroup:                      0 as u32,
                maxDynamicUniformBuffersPerPipelineLayout:    0 as u32,
                maxDynamicStorageBuffersPerPipelineLayout:    0 as u32,
                maxSampledTexturesPerShaderStage:             0 as u32,
                maxSamplersPerShaderStage:                    0 as u32,
                maxStorageBuffersPerShaderStage:              0 as u32,
                maxStorageTexturesPerShaderStage:             0 as u32,
                maxUniformBuffersPerShaderStage:              0 as u32,
                maxUniformBufferBindingSize:                  0 as u64,
                maxStorageBufferBindingSize:                  0 as u64,
                minUniformBufferOffsetAlignment:              0 as u32,
                minStorageBufferOffsetAlignment:              0 as u32,
                maxVertexBuffers:                             0 as u32,
                maxBufferSize:                                0 as u64,
                maxVertexAttributes:                          0 as u32,
                maxVertexBufferArrayStride:                   0 as u32,
                maxInterStageShaderVariables:                 0 as u32,
                maxColorAttachments:                          0 as u32,
                maxColorAttachmentBytesPerSample:             0 as u32,
                maxComputeWorkgroupStorageSize:               0 as u32,
                maxComputeInvocationsPerWorkgroup:            0 as u32,
                maxComputeWorkgroupSizeX:                     0 as u32,
                maxComputeWorkgroupSizeY:                     0 as u32,
                maxComputeWorkgroupSizeZ:                     0 as u32,
                maxComputeWorkgroupsPerDimension:             0 as u32,
                maxImmediateSize:                             0 as u32,
            }
            wgpuDeviceGetLimits(this.handle as WGPUDevice, &raw)
            let l = new Limits()
            l.maxTextureDimension1D                     = raw.maxTextureDimension1D as i64
            l.maxTextureDimension2D                     = raw.maxTextureDimension2D as i64
            l.maxTextureDimension3D                     = raw.maxTextureDimension3D as i64
            l.maxTextureArrayLayers                     = raw.maxTextureArrayLayers as i64
            l.maxBindGroups                             = raw.maxBindGroups as i64
            l.maxBindGroupsPlusVertexBuffers            = raw.maxBindGroupsPlusVertexBuffers as i64
            l.maxBindingsPerBindGroup                   = raw.maxBindingsPerBindGroup as i64
            l.maxDynamicUniformBuffersPerPipelineLayout = raw.maxDynamicUniformBuffersPerPipelineLayout as i64
            l.maxDynamicStorageBuffersPerPipelineLayout = raw.maxDynamicStorageBuffersPerPipelineLayout as i64
            l.maxSampledTexturesPerShaderStage          = raw.maxSampledTexturesPerShaderStage as i64
            l.maxSamplersPerShaderStage                 = raw.maxSamplersPerShaderStage as i64
            l.maxStorageBuffersPerShaderStage           = raw.maxStorageBuffersPerShaderStage as i64
            l.maxStorageTexturesPerShaderStage          = raw.maxStorageTexturesPerShaderStage as i64
            l.maxUniformBuffersPerShaderStage           = raw.maxUniformBuffersPerShaderStage as i64
            l.maxUniformBufferBindingSize               = raw.maxUniformBufferBindingSize
            l.maxStorageBufferBindingSize               = raw.maxStorageBufferBindingSize
            l.minUniformBufferOffsetAlignment           = raw.minUniformBufferOffsetAlignment as i64
            l.minStorageBufferOffsetAlignment           = raw.minStorageBufferOffsetAlignment as i64
            l.maxVertexBuffers                          = raw.maxVertexBuffers as i64
            l.maxBufferSize                             = raw.maxBufferSize
            l.maxVertexAttributes                       = raw.maxVertexAttributes as i64
            l.maxVertexBufferArrayStride                = raw.maxVertexBufferArrayStride as i64
            l.maxInterStageShaderVariables              = raw.maxInterStageShaderVariables as i64
            l.maxColorAttachments                       = raw.maxColorAttachments as i64
            l.maxColorAttachmentBytesPerSample          = raw.maxColorAttachmentBytesPerSample as i64
            l.maxComputeWorkgroupStorageSize            = raw.maxComputeWorkgroupStorageSize as i64
            l.maxComputeInvocationsPerWorkgroup         = raw.maxComputeInvocationsPerWorkgroup as i64
            l.maxComputeWorkgroupSizeX                  = raw.maxComputeWorkgroupSizeX as i64
            l.maxComputeWorkgroupSizeY                  = raw.maxComputeWorkgroupSizeY as i64
            l.maxComputeWorkgroupSizeZ                  = raw.maxComputeWorkgroupSizeZ as i64
            l.maxComputeWorkgroupsPerDimension          = raw.maxComputeWorkgroupsPerDimension as i64
            l.maxImmediateSize                          = raw.maxImmediateSize as i64
            l
        }

        /// Adapter description for the adapter that produced
        /// this device. The wgpu-allocated `WGPUStringView`
        /// payloads are copied into owned ilang `string`s and
        /// freed via `wgpuAdapterInfoFreeMembers` before return.
        pub getAdapterInfo(): AdapterInfo {
            let info = WGPUAdapterInfo {
                nextInChain:     0 as *void,
                vendor:          WGPUStringView { data: 0 as *void, length: 0 as u64 },
                architecture:    WGPUStringView { data: 0 as *void, length: 0 as u64 },
                device:          WGPUStringView { data: 0 as *void, length: 0 as u64 },
                description:     WGPUStringView { data: 0 as *void, length: 0 as u64 },
                backendType:     BackendType.undefined,
                adapterType:     AdapterType.unknown,
                vendorID:        0 as u32,
                deviceID:        0 as u32,
                subgroupMinSize: 0 as u32,
                subgroupMaxSize: 0 as u32,
            }
            wgpuDeviceGetAdapterInfo(this.handle as WGPUDevice, &info)
            let out = new AdapterInfo()
            if info.vendor.data       as i64 != 0 { out.vendor       = stringFromCstr(info.vendor.data as *const char) }
            if info.architecture.data as i64 != 0 { out.architecture = stringFromCstr(info.architecture.data as *const char) }
            if info.device.data       as i64 != 0 { out.device       = stringFromCstr(info.device.data as *const char) }
            if info.description.data  as i64 != 0 { out.description  = stringFromCstr(info.description.data as *const char) }
            out.backendType     = info.backendType
            out.adapterType     = info.adapterType
            out.vendorID        = info.vendorID as i64
            out.deviceID        = info.deviceID as i64
            out.subgroupMinSize = info.subgroupMinSize as i64
            out.subgroupMaxSize = info.subgroupMaxSize as i64
            wgpuAdapterInfoFreeMembers(info)
            out
        }
    }

    /// Value copy of `WGPULimits` — every field as plain ilang
    /// numeric primitives, no `nextInChain` exposed. Built by
    /// `Device.getLimits` / `Adapter.getLimits`.
    pub class Limits {
        pub maxTextureDimension1D:                        i64
        pub maxTextureDimension2D:                        i64
        pub maxTextureDimension3D:                        i64
        pub maxTextureArrayLayers:                        i64
        pub maxBindGroups:                                i64
        pub maxBindGroupsPlusVertexBuffers:               i64
        pub maxBindingsPerBindGroup:                      i64
        pub maxDynamicUniformBuffersPerPipelineLayout:    i64
        pub maxDynamicStorageBuffersPerPipelineLayout:    i64
        pub maxSampledTexturesPerShaderStage:             i64
        pub maxSamplersPerShaderStage:                    i64
        pub maxStorageBuffersPerShaderStage:              i64
        pub maxStorageTexturesPerShaderStage:             i64
        pub maxUniformBuffersPerShaderStage:              i64
        pub maxUniformBufferBindingSize:                  u64
        pub maxStorageBufferBindingSize:                  u64
        pub minUniformBufferOffsetAlignment:              i64
        pub minStorageBufferOffsetAlignment:              i64
        pub maxVertexBuffers:                             i64
        pub maxBufferSize:                                u64
        pub maxVertexAttributes:                          i64
        pub maxVertexBufferArrayStride:                   i64
        pub maxInterStageShaderVariables:                 i64
        pub maxColorAttachments:                          i64
        pub maxColorAttachmentBytesPerSample:             i64
        pub maxComputeWorkgroupStorageSize:               i64
        pub maxComputeInvocationsPerWorkgroup:            i64
        pub maxComputeWorkgroupSizeX:                     i64
        pub maxComputeWorkgroupSizeY:                     i64
        pub maxComputeWorkgroupSizeZ:                     i64
        pub maxComputeWorkgroupsPerDimension:             i64
        pub maxImmediateSize:                             i64

        pub init() {
            this.maxTextureDimension1D                     = 0
            this.maxTextureDimension2D                     = 0
            this.maxTextureDimension3D                     = 0
            this.maxTextureArrayLayers                     = 0
            this.maxBindGroups                             = 0
            this.maxBindGroupsPlusVertexBuffers            = 0
            this.maxBindingsPerBindGroup                   = 0
            this.maxDynamicUniformBuffersPerPipelineLayout = 0
            this.maxDynamicStorageBuffersPerPipelineLayout = 0
            this.maxSampledTexturesPerShaderStage          = 0
            this.maxSamplersPerShaderStage                 = 0
            this.maxStorageBuffersPerShaderStage           = 0
            this.maxStorageTexturesPerShaderStage          = 0
            this.maxUniformBuffersPerShaderStage           = 0
            this.maxUniformBufferBindingSize               = 0 as u64
            this.maxStorageBufferBindingSize               = 0 as u64
            this.minUniformBufferOffsetAlignment           = 0
            this.minStorageBufferOffsetAlignment           = 0
            this.maxVertexBuffers                          = 0
            this.maxBufferSize                             = 0 as u64
            this.maxVertexAttributes                       = 0
            this.maxVertexBufferArrayStride                = 0
            this.maxInterStageShaderVariables              = 0
            this.maxColorAttachments                       = 0
            this.maxColorAttachmentBytesPerSample          = 0
            this.maxComputeWorkgroupStorageSize            = 0
            this.maxComputeInvocationsPerWorkgroup         = 0
            this.maxComputeWorkgroupSizeX                  = 0
            this.maxComputeWorkgroupSizeY                  = 0
            this.maxComputeWorkgroupSizeZ                  = 0
            this.maxComputeWorkgroupsPerDimension          = 0
            this.maxImmediateSize                          = 0
        }

    }

    /// Adapter description as plain ilang values. Built by
    /// `Device.getAdapterInfo`; the wgpu-allocated string
    /// payloads are copied into owned ilang strings before
    /// `wgpuAdapterInfoFreeMembers` is called.
    pub class AdapterInfo {
        pub vendor:          string
        pub architecture:    string
        pub device:          string
        pub description:     string
        pub backendType:     BackendType
        pub adapterType:     AdapterType
        pub vendorID:        i64
        pub deviceID:        i64
        pub subgroupMinSize: i64
        pub subgroupMaxSize: i64

        pub init() {
            this.vendor          = ""
            this.architecture    = ""
            this.device          = ""
            this.description     = ""
            this.backendType     = BackendType.undefined
            this.adapterType     = AdapterType.unknown
            this.vendorID        = 0
            this.deviceID        = 0
            this.subgroupMinSize = 0
            this.subgroupMaxSize = 0
        }

    }
}
