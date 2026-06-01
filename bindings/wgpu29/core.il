/// wgpu binding — shared primitives. No `wgpu_native` symbols
/// here; every per-feature file imports the value-struct types
/// (`WGPUStringView` / `WGPUChainedStruct` / `WGPUColor` /
/// `WGPUExtent3D` / `WGPUOrigin3D`) and the `TextureFormat`
/// enum from this file.
///
/// Pinned to wgpu-native v29.0.0.0 (webgpu.h). Strings are
/// `WGPUStringView { const char* data; size_t length }` (16 B,
/// passed in 2 regs); callback signatures splay the view into
/// two `i64`s.

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
    rgb10A2Uint          = 0x1D
    rgb10A2Unorm         = 0x1E
    rg11B10Ufloat        = 0x1F
    rgb9E5Ufloat         = 0x20
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
    bc1RGBAUnorm         = 0x32
    bc1RGBAUnormSrgb     = 0x33
    bc2RGBAUnorm         = 0x34
    bc2RGBAUnormSrgb     = 0x35
    bc3RGBAUnorm         = 0x36
    bc3RGBAUnormSrgb     = 0x37
    bc4RUnorm            = 0x38
    bc4RSnorm            = 0x39
    bc5RGUnorm           = 0x3A
    bc5RGSnorm           = 0x3B
    bc6HRGBUfloat        = 0x3C
    bc6HRGBFloat         = 0x3D
    bc7RGBAUnorm         = 0x3E
    bc7RGBAUnormSrgb     = 0x3F
    etc2RGB8Unorm        = 0x40
    etc2RGB8UnormSrgb    = 0x41
    etc2RGB8A1Unorm      = 0x42
    etc2RGB8A1UnormSrgb  = 0x43
    etc2RGBA8Unorm       = 0x44
    etc2RGBA8UnormSrgb   = 0x45
    eacR11Unorm          = 0x46
    eacR11Snorm          = 0x47
    eacRG11Unorm         = 0x48
    eacRG11Snorm         = 0x49
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

pub enum CallbackMode: u32 {
    waitAnyOnly         = 0x01
    allowProcessEvents  = 0x02
    allowSpontaneous    = 0x03
}

/// `WGPUAdapterType` — adapter classification returned by
/// `wgpuAdapterGetInfo`. Lives in core (not `adapter.il`) so the
/// friendly `Device.getAdapterInfo` in `device.il` can name the
/// type without importing `adapter.il`.
pub enum AdapterType: u32 {
    discreteGPU   = 0x01
    integratedGPU = 0x02
    cpu           = 0x03
    unknown       = 0x04
}

/// `WGPUBackendType`. Same rationale as `AdapterType` — referenced
/// by both `adapter.il` (RequestAdapterOptions) and `device.il`
/// (getAdapterInfo / AdapterInfo value wrapper).
pub enum BackendType: u32 {
    undefined = 0x00
    null      = 0x01
    webgpu    = 0x02
    d3d11     = 0x03
    d3d12     = 0x04
    metal     = 0x05
    vulkan    = 0x06
    opengl    = 0x07
    opengles  = 0x08
}

pub enum ComponentSwizzle: u32 {
    undefined = 0x00
    zero      = 0x01
    one       = 0x02
    r         = 0x03
    g         = 0x04
    b         = 0x05
    a         = 0x06
}

pub enum FeatureName: u32 {
    coreFeaturesAndLimits         = 0x01
    depthClipControl              = 0x02
    depth32FloatStencil8          = 0x03
    textureCompressionBC          = 0x04
    textureCompressionBCSliced3D  = 0x05
    textureCompressionETC2        = 0x06
    textureCompressionASTC        = 0x07
    textureCompressionASTCSliced3D = 0x08
    timestampQuery                = 0x09
    indirectFirstInstance         = 0x0A
    shaderF16                     = 0x0B
    rG11B10UfloatRenderable       = 0x0C
    bGRA8UnormStorage             = 0x0D
    float32Filterable             = 0x0E
    float32Blendable              = 0x0F
    clipDistances                 = 0x10
    dualSourceBlending            = 0x11
    subgroups                     = 0x12
    textureFormatsTier1           = 0x13
    textureFormatsTier2           = 0x14
    primitiveIndex                = 0x15
    textureComponentSwizzle       = 0x16
}

pub enum MapAsyncStatus: u32 {
    success           = 0x01
    callbackCancelled = 0x02
    error             = 0x03
    aborted           = 0x04
}

pub enum OptionalBool: u32 {
    false_    = 0x00
    true_     = 0x01
    undefined = 0x02
}

pub enum PredefinedColorSpace: u32 {
    sRGB      = 0x01
    displayP3 = 0x02
}

pub enum Status: u32 {
    success = 0x01
    error   = 0x02
}

pub enum SType: u32 {
    shaderSourceSPIRV                = 0x01
    shaderSourceWGSL                 = 0x02
    renderPassMaxDrawCount           = 0x03
    surfaceSourceMetalLayer          = 0x04
    surfaceSourceWindowsHWND         = 0x05
    surfaceSourceXlibWindow          = 0x06
    surfaceSourceWaylandSurface      = 0x07
    surfaceSourceAndroidNativeWindow = 0x08
    surfaceSourceXCBWindow           = 0x09
    surfaceColorManagement           = 0x0A
    requestAdapterWebXROptions       = 0x0B
    textureComponentSwizzleDescriptor = 0x0C
    externalTextureBindingLayout     = 0x0D
    externalTextureBindingEntry      = 0x0E
    compatibilityModeLimits          = 0x0F
    textureBindingViewDimension      = 0x10
}

pub enum ToneMappingMode: u32 {
    standard = 0x01
    extended = 0x02
}

pub enum WaitStatus: u32 {
    success  = 0x01
    timedOut = 0x02
    error    = 0x03
}

@extern(C, "wgpu_native") {
    // ─── fundamental handles ──────────────────────────────────
    //
    // `WGPUDevice` and `WGPUQueue` live in core (not their own
    // `device.il`) so the per-resource files — `shader.il`,
    // `buffer.il`, `pipeline.il`, … — can declare
    // `wgpuDeviceCreate*(device: WGPUDevice, …)` without
    // importing `device.il`, which would otherwise cycle back
    // through the friendly `Device` class hosted there.
    @handle pub struct WGPUDevice {}
    @handle pub struct WGPUQueue {}

    // `WGPUInstance` and `WGPUAdapter` also live in core so the
    // friendly `Instance.requestAdapter` / `Adapter.requestDevice`
    // / `Buffer.mapAsync` / `Queue.onSubmittedWorkDone` callback-
    // pump helpers can name the handle types without forming
    // cycles between `instance.il` ↔ `adapter.il` ↔ `device.il`
    // ↔ `buffer.il` ↔ `queue.il`.
    @handle pub struct WGPUInstance {}
    @handle pub struct WGPUAdapter {}

    /// `const char* data + size_t length`. Empty label = `{ 0, 0 }`;
    /// `length = WGPU_STRLEN` (= u64::MAX) means "wgpu strlen the
    /// data".
    pub struct WGPUStringView {
        data:   *void
        length: u64
    }
    /// Pointer to the previous-in-chain sType-tagged struct.
    /// Used as the `nextInChain` head for every descriptor that
    /// supports extension chains.
    pub struct WGPUChainedStruct {
        next:  *void
        sType: SType
    }
    /// Clear-colour / blend-constant RGBA in 0..1.
    pub struct WGPUColor {
        r: f64
        g: f64
        b: f64
        a: f64
    }
    /// 3D dimension triple used by texture / copy ops.
    pub struct WGPUExtent3D {
        width:              u32
        height:             u32
        depthOrArrayLayers: u32
    }
    /// 3D origin used by texture / copy ops.
    pub struct WGPUOrigin3D {
        x: u32
        y: u32
        z: u32
    }

    /// Capability limits shared by `Adapter.GetLimits` /
    /// `Device.GetLimits`. Every field is an out-param —
    /// pre-fill `nextInChain = 0 as *void` and pass `&limits`;
    /// wgpu sets the rest. `u32` fields default to
    /// `WGPU_LIMIT_U32_UNDEFINED` (0xFFFFFFFF) when not
    /// applicable, `u64` to `WGPU_LIMIT_U64_UNDEFINED`
    /// (0xFFFFFFFFFFFFFFFF).
    pub struct WGPULimits {
        nextInChain:                                  *void
        maxTextureDimension1D:                        u32
        maxTextureDimension2D:                        u32
        maxTextureDimension3D:                        u32
        maxTextureArrayLayers:                        u32
        maxBindGroups:                                u32
        maxBindGroupsPlusVertexBuffers:               u32
        maxBindingsPerBindGroup:                      u32
        maxDynamicUniformBuffersPerPipelineLayout:    u32
        maxDynamicStorageBuffersPerPipelineLayout:    u32
        maxSampledTexturesPerShaderStage:             u32
        maxSamplersPerShaderStage:                    u32
        maxStorageBuffersPerShaderStage:              u32
        maxStorageTexturesPerShaderStage:             u32
        maxUniformBuffersPerShaderStage:              u32
        maxUniformBufferBindingSize:                  u64
        maxStorageBufferBindingSize:                  u64
        minUniformBufferOffsetAlignment:              u32
        minStorageBufferOffsetAlignment:              u32
        maxVertexBuffers:                             u32
        maxBufferSize:                                u64
        maxVertexAttributes:                          u32
        maxVertexBufferArrayStride:                   u32
        maxInterStageShaderVariables:                 u32
        maxColorAttachments:                          u32
        maxColorAttachmentBytesPerSample:             u32
        maxComputeWorkgroupStorageSize:               u32
        maxComputeInvocationsPerWorkgroup:            u32
        maxComputeWorkgroupSizeX:                     u32
        maxComputeWorkgroupSizeY:                     u32
        maxComputeWorkgroupSizeZ:                     u32
        maxComputeWorkgroupsPerDimension:             u32
        maxImmediateSize:                             u32
    }

    // ─── adapter info + supported features ───────────────────
    //
    // Live in core (not `adapter.il`) so `device.il`'s
    // `getAdapterInfo` and the friendly `AdapterInfo` value
    // wrapper there don't need to import `adapter.il` — which
    // would otherwise cycle via `adapter.il → device.il`
    // (`Adapter.requestDevice` returns the friendly `Device`).
    //
    // `wgpuAdapterInfoFreeMembers` must be called on the value
    // returned by `wgpuAdapterGetInfo` / `wgpuDeviceGetAdapterInfo`
    // to release the `WGPUStringView` payload allocations.
    pub struct WGPUAdapterInfo {
        nextInChain:     *void
        vendor:          WGPUStringView
        architecture:    WGPUStringView
        device:          WGPUStringView
        description:     WGPUStringView
        backendType:     BackendType
        adapterType:     AdapterType
        vendorID:        u32
        deviceID:        u32
        subgroupMinSize: u32
        subgroupMaxSize: u32
    }
    /// Wgpu-allocated array; free with
    /// `wgpuSupportedFeaturesFreeMembers` (declared in
    /// `adapter.il`).
    pub struct WGPUSupportedFeatures {
        featureCount: u64
        features:     *void          // WGPUFeatureName const*
    }
    @lib pub fn wgpuAdapterInfoFreeMembers(info: WGPUAdapterInfo)

    // ─── instance event pump ─────────────────────────────────
    //
    // Lives in core so `buffer.il` / `queue.il` can drive the
    // callback dispatcher inside their `mapAsync` /
    // `onSubmittedWorkDone` sync helpers without importing
    // `instance.il` (which would form a cycle through
    // `instance.il → adapter.il → device.il → buffer.il`).
    @lib pub fn wgpuInstanceProcessEvents(instance: WGPUInstance)
}
