/// `WGPUAdapter` + `wgpuInstanceRequestAdapter` async plumbing.
///
/// `compatibleSurface` in `WGPURequestAdapterOptions` is typed
/// as `*void` to break the adapter ↔ surface module cycle —
/// pass `surface.handle as *void` (or `0 as *void` to skip the
/// surface compatibility check).

use instance { WGPUInstance }

@extern(C, "wgpu_native") {
    @handle pub struct WGPUAdapter {}

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
