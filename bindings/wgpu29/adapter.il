/// `WGPUAdapter` + `wgpuInstanceRequestAdapter` async plumbing.
///
/// `compatibleSurface` in `WGPURequestAdapterOptions` is typed
/// as `*void` to break the adapter ↔ surface module cycle —
/// pass `surface.handle as *void` (or `0 as *void` to skip the
/// surface compatibility check).

use core { WGPUStringView, WGPULimits, FeatureName }
use instance { WGPUInstance }
use std.ffi { cstrFromString }

pub enum AdapterType: u32 {
    discreteGPU   = 0x01
    integratedGPU = 0x02
    cpu           = 0x03
    unknown       = 0x04
}

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
        backendType:     BackendType
        adapterType:     AdapterType
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

    // ─── friendly wrapper ─────────────────────────────────────

    /// Wraps `WGPUAdapter`. `requestDevice` is callback-based;
    /// use the raw `wgpuAdapterRequestDevice` directly. `getInfo`
    /// / `getLimits` / `getFeatures` need the value-wrapper
    /// classes from device.il — call them through the umbrella's
    /// `AdapterInfo` / `Limits` helpers.
    pub class Adapter {
        pub handle: i64

        pub init(handle: WGPUAdapter) {
            this.handle = handle as i64
        }
        pub release() {
            if this.handle != 0 {
                wgpuAdapterRelease(this.handle as WGPUAdapter)
                this.handle = 0
            }
        }
        /// `wgpuAdapterHasFeature` returns `WGPUBool` (u32 0/1);
        /// this method casts to plain `bool` for the caller.
        pub hasFeature(feature: FeatureName): bool {
            wgpuAdapterHasFeature(this.handle as WGPUAdapter, feature as u32) != 0
        }
    }
}
