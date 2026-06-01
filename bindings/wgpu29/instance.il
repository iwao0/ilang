/// `WGPUInstance` + logging + `wgpuCreateInstance` /
/// `wgpuInstanceProcessEvents`. The instance is the root object
/// every other wgpu resource hangs off; create exactly one per
/// process and reuse it across surfaces / devices.

use core { WGPUStringView, WGPUInstance, CallbackMode }
use std.ffi { cstrFromString }

pub enum InstanceFeatureName: u32 {
    timedWaitAny              = 0x01
    shaderSourceSPIRV         = 0x02
    multipleDevicesPerAdapter = 0x03
}

pub enum WGSLLanguageFeatureName: u32 {
    readonlyAndReadwriteStorageTextures = 0x01
    packed4x8IntegerDotProduct          = 0x02
    unrestrictedPointerParameters       = 0x03
    pointerCompositeAccess              = 0x04
    uniformBufferStandardLayout         = 0x05
    subgroupId                          = 0x06
    textureAndSamplerLet                = 0x07
    subgroupUniformity                  = 0x08
    textureFormatsTier1                 = 0x09
}

pub enum NativeSType: u32 {
    deviceExtras                = 0x00030001
    nativeLimits                = 0x00030002
    pipelineLayoutExtras        = 0x00030003
    shaderSourceGLSL            = 0x00030004
    instanceExtras              = 0x00030006
    bindGroupEntryExtras        = 0x00030007
    bindGroupLayoutEntryExtras  = 0x00030008
    querySetDescriptorExtras    = 0x00030009
    surfaceConfigurationExtras  = 0x0003000A
    surfaceSourceSwapChainPanel = 0x0003000B
    primitiveStateExtras        = 0x0003000C
}

pub enum NativeFeature: u64 {
    immediates                                            = 0x00030001
    textureAdapterSpecificFormatFeatures                  = 0x00030002
    multiDrawIndirectCount                                = 0x00030004
    vertexWritableStorage                                 = 0x00030005
    textureBindingArray                                   = 0x00030006
    sampledTextureAndStorageBufferArrayNonUniformIndexing = 0x00030007
    pipelineStatisticsQuery                               = 0x00030008
    storageResourceBindingArray                           = 0x00030009
    partiallyBoundBindingArray                            = 0x0003000A
    textureFormat16bitNorm                                = 0x0003000B
    textureCompressionAstcHdr                             = 0x0003000C
    mappablePrimaryBuffers                                = 0x0003000E
    bufferBindingArray                                    = 0x0003000F
    uniformBufferAndStorageTextureArrayNonUniformIndexing = 0x00030010
    polygonModeLine                                       = 0x00030013
    polygonModePoint                                      = 0x00030014
    conservativeRasterization                             = 0x00030015
    spirvShaderPassthrough                                = 0x00030017
    vertexAttribute64bit                                  = 0x00030019
    textureFormatNv12                                     = 0x0003001A
    rayQuery                                              = 0x0003001C
    shaderF64                                             = 0x0003001D
    shaderI16                                             = 0x0003001E
    shaderEarlyDepthTest                                  = 0x00030020
    subgroup                                              = 0x00030021
    subgroupVertex                                        = 0x00030022
    subgroupBarrier                                       = 0x00030023
    timestampQueryInsideEncoders                          = 0x00030024
    timestampQueryInsidePasses                            = 0x00030025
    shaderInt64                                           = 0x00030026
}

pub enum LogLevel: u32 {
    off   = 0x00
    error = 0x01
    warn  = 0x02
    info  = 0x03
    debug = 0x04
    trace = 0x05
}

pub enum Dx12Compiler: u32 {
    undefined = 0x00
    fxc       = 0x01
    dxc       = 0x02
}

pub enum Gles3MinorVersion: u32 {
    automatic = 0x00
    version0  = 0x01
    version1  = 0x02
    version2  = 0x03
}

pub enum DxcMaxShaderModel: u32 {
    v6_0 = 0x00
    v6_1 = 0x01
    v6_2 = 0x02
    v6_3 = 0x03
    v6_4 = 0x04
    v6_5 = 0x05
    v6_6 = 0x06
    v6_7 = 0x07
}

pub enum GLFenceBehaviour: u32 {
    normal     = 0x00
    autoFinish = 0x01
}

pub enum Dx12SwapchainKind: u32 {
    undefined       = 0x00
    dxgiFromHwnd    = 0x01
    dxgiFromVisual  = 0x02
}

pub enum NativeDisplayHandleType: u32 {
    none    = 0x00
    xlib    = 0x01
    xcb     = 0x02
    wayland = 0x03
}

pub enum NativeTextureFormat: u32 {
    r16Unorm    = 0x00030001
    r16Snorm    = 0x00030002
    rg16Unorm   = 0x00030003
    rg16Snorm   = 0x00030004
    rgba16Unorm = 0x00030005
    rgba16Snorm = 0x00030006
    nV12        = 0x00030007
    p010        = 0x00030008
}

@flags
pub enum InstanceBackend: u64 {
    all           = 0x00
    vulkan        = 0x01
    gl            = 0x02
    metal         = 0x04
    dX12          = 0x08
    browserWebGPU = 0x20
    primary       = 0x2D
    secondary     = 0x02
}

@flags
pub enum InstanceFlag: u64 {
    empty                              = 0x00
    debug                              = 0x01
    validation                         = 0x02
    discardHalLabels                   = 0x04
    allowUnderlyingNoncompliantAdapter = 0x08
    gPUBasedValidation                 = 0x10
    validationIndirectCall             = 0x20
    automaticTimestampNormalization    = 0x40
    default                            = 0x01000000
    debugging                          = 0x02000000
    advancedDebugging                  = 0x04000000
    withEnv                            = 0x08000000
}

// The sync-wrapper module-level OUT slot + `_wgpuOnRequestAdapter`
// callback now live in `adapter_class.il` next to the top-level
// `requestAdapter(inst: Instance, ...)` helper.

@extern(C, "wgpu_native") {
    // `WGPUInstance` lives in core.il — see the rationale next to
    // its declaration there.

    // ─── logging (wgpu.h native extension) ────────────────────
    // WGPULogLevel: Off=0 Error=1 Warn=2 Info=3 Debug=4 Trace=5.
    // callback: void(WGPULogLevel, WGPUStringView message, void*)
    // → (level i32, msg.data i64, msg.len i64, userdata i64).
    @lib pub fn wgpuSetLogLevel(level: u32)
    @lib pub fn wgpuSetLogCallback(
        callback: fn(i32, i64, i64, i64), userdata: *void
    )

    // `descriptor` is nullable — pass `0 as *void` for the
    // default `WGPUInstanceDescriptor`.
    @lib pub fn wgpuCreateInstance(descriptor: *void): WGPUInstance

    // `wgpuInstanceProcessEvents` lives in core.il (so
    // `buffer.il` / `queue.il` can call it without cycling back
    // through this file).
    @lib pub fn wgpuInstanceAddRef(instance: WGPUInstance)
    @lib pub fn wgpuInstanceRelease(instance: WGPUInstance)

    // ─── instance-level feature / limit query ─────────────────
    //
    // These are global (no instance handle required) — they probe
    // the wgpu-native build, not a particular adapter / device.
    // Free the wgpu-allocated array via the matching FreeMembers
    // helper.
    pub struct WGPUSupportedInstanceFeatures {
        featureCount: u64
        features:     *void          // WGPUInstanceFeatureName const*
    }
    pub struct WGPUInstanceLimits {
        nextInChain:          *void
        timedWaitAnyMaxCount: u64
    }
    @lib pub fn wgpuGetInstanceFeatures(features: *WGPUSupportedInstanceFeatures)
    @lib pub fn wgpuGetInstanceLimits(limits: *WGPUInstanceLimits): u32   // WGPUStatus
    @lib pub fn wgpuHasInstanceFeature(feature: u32): u32                  // WGPUInstanceFeatureName → WGPUBool
    @lib pub fn wgpuSupportedInstanceFeaturesFreeMembers(features: WGPUSupportedInstanceFeatures)

    // ─── WGSL language feature query ──────────────────────────
    pub struct WGPUSupportedWGSLLanguageFeatures {
        featureCount: u64
        features:     *void          // WGPUWGSLLanguageFeatureName const*
    }
    @lib pub fn wgpuInstanceGetWGSLLanguageFeatures(
        instance: WGPUInstance, features: *WGPUSupportedWGSLLanguageFeatures
    )
    @lib pub fn wgpuInstanceHasWGSLLanguageFeature(
        instance: WGPUInstance, feature: u32        // WGPUWGSLLanguageFeatureName
    ): u32                                          // WGPUBool
    @lib pub fn wgpuSupportedWGSLLanguageFeaturesFreeMembers(
        features: WGPUSupportedWGSLLanguageFeatures
    )

    // ─── WGPUFuture wait + proc table lookup ──────────────────
    //
    // `WaitAny` blocks the calling thread until at least one of
    // the supplied futures resolves, or `timeoutNS` elapses.
    // Each `WGPUFuture` carries a `u64 id`; the matching
    // `WGPUFutureWaitInfo.completed` flag is written by wgpu
    // when that future fires.
    // WGPUWaitStatus: Success=1, TimedOut=2, UnsupportedTimeout=3,
    //                 UnsupportedCount=4, UnsupportedMixedSources=5.
    pub struct WGPUFutureWaitInfo {
        future:    u64                              // WGPUFuture id
        completed: u32                              // WGPUBool
    }
    @lib pub fn wgpuInstanceWaitAny(
        instance:    WGPUInstance,
        futureCount: u64,
        futures:     *WGPUFutureWaitInfo,
        timeoutNS:   u64
    ): u32                                          // WGPUWaitStatus

    // Returns the C function pointer matching `procName`, or
    // null if the proc isn't exported by the current wgpu_native
    // build. Useful for symbol probing on platforms whose dlopen
    // is unreliable for missing entry points.
    @lib pub fn wgpuGetProcAddress(procName: WGPUStringView): *void

    // wgpu-native extension. Returns the loaded library's
    // version as a packed u32 (major / minor / patch / build).
    @lib pub fn wgpuGetVersion(): u32

    // ─── wgpu-native diagnostic report ────────────────────────
    //
    // Pre-fill `report = new WGPUGlobalReport()` and pass `&report`;
    // wgpu fills in per-resource registry counters across every
    // backend hub. Useful for spotting leaks (`numAllocated -
    // numReleasedFromUser` should converge).
    pub struct WGPURegistryReport {
        numAllocated:        u64
        numKeptFromUser:     u64
        numReleasedFromUser: u64
        elementSize:         u64
    }
    pub struct WGPUHubReport {
        adapters:          WGPURegistryReport
        devices:           WGPURegistryReport
        queues:            WGPURegistryReport
        pipelineLayouts:   WGPURegistryReport
        shaderModules:     WGPURegistryReport
        bindGroupLayouts:  WGPURegistryReport
        bindGroups:        WGPURegistryReport
        commandBuffers:    WGPURegistryReport
        renderBundles:     WGPURegistryReport
        renderPipelines:   WGPURegistryReport
        computePipelines:  WGPURegistryReport
        pipelineCaches:    WGPURegistryReport
        querySets:         WGPURegistryReport
        buffers:           WGPURegistryReport
        textures:          WGPURegistryReport
        textureViews:      WGPURegistryReport
        samplers:          WGPURegistryReport
    }
    pub struct WGPUGlobalReport {
        surfaces: WGPURegistryReport
        hub:      WGPUHubReport
    }
    @lib pub fn wgpuGenerateReport(instance: WGPUInstance, report: *WGPUGlobalReport)

    // ─── wgpu-native synchronous adapter enumeration ─────────
    //
    // Synchronous alternative to `wgpuInstanceRequestAdapter`.
    // Pass `adapters = 0 as *WGPUAdapter` (null) to count first,
    // then allocate and call again to fill. `backends` is a
    // bitmask of `WGPUInstanceBackend` (BrowserWebGPU=0x10,
    // Vulkan=0x01, Metal=0x02, DX12=0x04, OpenGL=0x40, etc.) —
    // pass 0 for "all".
    pub struct WGPUInstanceEnumerateAdapterOptions {
        nextInChain: *const void
        backends:    InstanceBackend
    }
    @lib pub fn wgpuInstanceEnumerateAdapters(
        instance: WGPUInstance,
        options:  *WGPUInstanceEnumerateAdapterOptions,
        adapters: *void                       // out: *WGPUAdapter array; null to count
    ): u64
}

// `Instance` wrapper class lives in `instance_class.il`. The
// `requestAdapter` synchronous helper moved to a top-level fn in
// `adapter_class.il` — keeping it as a method on `Instance` would
// require `instance_class.il → adapter_class.il → device_class.il
// → buffer_class.il → instance_class.il` (cycle).
