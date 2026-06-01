/// `WGPUAdapter` + `wgpuInstanceRequestAdapter` async plumbing.
///
/// `compatibleSurface` in `WGPURequestAdapterOptions` is typed
/// as `*void` to break the adapter ↔ surface module cycle —
/// pass `surface.handle as *void` (or `0 as *void` to skip the
/// surface compatibility check).

use core { WGPUStringView, WGPULimits }
use instance { WGPUInstance }

@extern(C, "wgpu_native") {
    @handle pub struct WGPUAdapter {}

    // ─── introspection ────────────────────────────────────────
    //
    // WGPUAdapterType: DiscreteGPU=1, IntegratedGPU=2, CPU=3,
    //                  Unknown=4.
    // WGPUBackendType: Undefined=0, Null=1, WebGPU=2, D3D11=3,
    //                  D3D12=4, Metal=5, Vulkan=6, OpenGL=7,
    //                  OpenGLES=8.
    // `wgpuAdapterInfoFreeMembers` must be called on the value
    // returned by GetInfo to release the StringView allocations.
    pub struct WGPUAdapterInfo {
        nextInChain:     *void
        vendor:          WGPUStringView
        architecture:    WGPUStringView
        device:          WGPUStringView
        description:     WGPUStringView
        backendType:     u32
        adapterType:     u32
        vendorID:        u32
        deviceID:        u32
        subgroupMinSize: u32
        subgroupMaxSize: u32
    }
    /// Wgpu-allocated array; `features` is owned by the adapter
    /// info struct it came in, no separate free is required.
    pub struct WGPUSupportedFeatures {
        featureCount: u64
        features:     *void          // WGPUFeatureName const*
    }
    // WGPUStatus: Success=1, Error=2.
    @lib pub fn wgpuAdapterGetInfo(
        adapter: WGPUAdapter, info: *WGPUAdapterInfo
    ): u32
    @lib pub fn wgpuAdapterGetLimits(
        adapter: WGPUAdapter, limits: *WGPULimits
    ): u32
    @lib pub fn wgpuAdapterGetFeatures(
        adapter: WGPUAdapter, features: *WGPUSupportedFeatures
    )
    @lib pub fn wgpuAdapterHasFeature(
        adapter: WGPUAdapter, feature: u32         // WGPUFeatureName
    ): u32                                          // WGPUBool
    @lib pub fn wgpuAdapterInfoFreeMembers(info: WGPUAdapterInfo)
    @lib pub fn wgpuAdapterAddRef(adapter: WGPUAdapter)
    @lib pub fn wgpuAdapterRelease(adapter: WGPUAdapter)
    // Free the wgpu-allocated `features` array inside a
    // populated SupportedFeatures struct. Call after copying out
    // the values you need.
    @lib pub fn wgpuSupportedFeaturesFreeMembers(features: WGPUSupportedFeatures)

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

    // All enum/bool fields are 4 B; `compatibleSurface` makes
    // the chosen adapter able to present to that surface
    // (required before `wgpuSurfaceConfigure`, otherwise
    // "invalid device for surface").
    // featureLevel / powerPreference / backendType Undefined = 0.
    pub struct WGPURequestAdapterOptions {
        nextInChain:          *void
        featureLevel:         u32
        powerPreference:      u32
        forceFallbackAdapter: u32     // WGPUBool
        backendType:          u32
        compatibleSurface:    *void   // WGPUSurface or null
    }

    @lib pub fn wgpuInstanceRequestAdapter(
        instance:     WGPUInstance,
        options:      *void,
        callbackInfo: WGPURequestAdapterCallbackInfo
    ): u64
}
