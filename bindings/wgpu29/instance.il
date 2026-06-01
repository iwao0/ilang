/// `WGPUInstance` + logging + `wgpuCreateInstance` /
/// `wgpuInstanceProcessEvents`. The instance is the root object
/// every other wgpu resource hangs off; create exactly one per
/// process and reuse it across surfaces / devices.

use core { WGPUStringView }

@extern(C, "wgpu_native") {
    @handle pub struct WGPUInstance {}

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

    // Drive any pending callbacks registered via
    // `AllowProcessEvents` mode. Call between request* and
    // reading the callback's output cell.
    @lib pub fn wgpuInstanceProcessEvents(instance: WGPUInstance)
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
}
