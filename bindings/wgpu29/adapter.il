/// `WGPUAdapter` + `wgpuInstanceRequestAdapter` async plumbing.
///
/// `compatibleSurface` in `WGPURequestAdapterOptions` is typed
/// as `*void` to break the adapter ↔ surface module cycle —
/// pass `surface.handle as *void` (or `0 as *void` to skip the
/// surface compatibility check).

use core {
    WGPULimits,
    WGPUInstance, WGPUAdapter,
    WGPUAdapterInfo, WGPUSupportedFeatures,
    BackendType, CallbackMode,
}

pub enum FeatureLevel: u32 {
    undefined     = 0x00
    compatibility = 0x01
    core          = 0x02
}

pub enum PowerPreference: u32 {
    undefined       = 0x00
    lowPower        = 0x01
    highPerformance = 0x02
}

pub enum RequestAdapterStatus: u32 {
    success           = 0x01
    callbackCancelled = 0x02
    unavailable       = 0x03
    error             = 0x04
}

pub enum RequestDeviceStatus: u32 {
    success           = 0x01
    callbackCancelled = 0x02
    error             = 0x03
}

// The synchronous `Adapter.requestDevice` callback slot lives in
// `adapter_class.il` alongside the friendly `Adapter` wrapper.

@extern(C, "wgpu_native") {
    // `WGPUAdapter`, `WGPUAdapterInfo`, `WGPUSupportedFeatures`,
    // `AdapterType`, `BackendType`, and `wgpuAdapterInfoFreeMembers`
    // live in core.il — see the rationale next to their
    // declarations there.

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
    // `wgpuAdapterInfoFreeMembers` is declared in core.il.
    @lib pub fn wgpuAdapterAddRef(adapter: WGPUAdapter)
    @lib pub fn wgpuAdapterRelease(adapter: WGPUAdapter)
    // Free the wgpu-allocated `features` array inside a
    // populated SupportedFeatures struct. Call after copying out
    // the values you need.
    @lib pub fn wgpuSupportedFeaturesFreeMembers(features: WGPUSupportedFeatures)

    // `callback` shape mirrors
    //   void(WGPURequestAdapterStatus status, WGPUAdapter adapter,
    //        WGPUStringView message, void* ud1, void* ud2)
    // with `message` splayed into (data, len) i64s.
    pub struct WGPURequestAdapterCallbackInfo {
        nextInChain: *void
        mode:        CallbackMode
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
        featureLevel:         FeatureLevel
        powerPreference:      PowerPreference
        forceFallbackAdapter: u32     // WGPUBool
        backendType:          BackendType
        compatibleSurface:    *void   // WGPUSurface or null
    }

    @lib pub fn wgpuInstanceRequestAdapter(
        instance:     WGPUInstance,
        options:      *void,
        callbackInfo: WGPURequestAdapterCallbackInfo
    ): u64
}

// `Adapter` wrapper class + top-level `requestAdapter(inst, ...)`
// helper live in `adapter_class.il`.
