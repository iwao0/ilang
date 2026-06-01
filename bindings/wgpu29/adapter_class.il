/// Friendly `Adapter` wrapper + the top-level sync helper
/// `requestAdapter(inst: Instance, ...)` pulled out of
/// `adapter.il` and `instance.il`.
///
/// `Instance.requestAdapter` is NOT a method on `Instance` —
/// keeping it there would force
///   instance_class.il → adapter_class.il → device_class.il
///                     → buffer_class.il  → instance_class.il
/// (cycle). The free function form `requestAdapter(inst, ...)`
/// breaks that loop while preserving the synchronous wgpu-
/// callback-pump semantics.

use core {
    WGPUInstance, WGPUAdapter, WGPUDevice,
    wgpuInstanceProcessEvents, CallbackMode, BackendType,
    FeatureName,
}
use adapter {
    PowerPreference, FeatureLevel,
    WGPURequestAdapterOptions, WGPURequestAdapterCallbackInfo,
    wgpuInstanceRequestAdapter, wgpuAdapterRelease, wgpuAdapterHasFeature,
}
use device {
    WGPURequestDeviceCallbackInfo, wgpuAdapterRequestDevice,
}
use device_class { Device }
use instance_class { Instance }

// ─── sync wrapper — module-level OUT slots + C callbacks ──────
//
// `_wgpuRequestAdapterOut` backs the top-level `requestAdapter`;
// `_wgpuRequestDeviceOut` backs `Adapter.requestDevice`. Reset,
// post, pump `wgpuInstanceProcessEvents` until filled.
let _wgpuRequestAdapterOut: i64[] = [0]
let _wgpuRequestDeviceOut:  i64[] = [0]

@extern(C) {
    pub fn _wgpuOnRequestAdapter(
        status: i32, adapter: i64, msgData: i64, msgLen: i64,
        ud1: i64, ud2: i64
    ) {
        _wgpuRequestAdapterOut[0] = adapter
    }
    pub fn _wgpuOnRequestDevice(
        status: i32, device: i64, msgData: i64, msgLen: i64,
        ud1: i64, ud2: i64
    ) {
        _wgpuRequestDeviceOut[0] = device
    }
}

@extern(C, "wgpu_native") {
/// Synchronously request an adapter. Internally pumps
/// `wgpuInstanceProcessEvents` in a loop until the wgpu
/// callback fires and writes the adapter handle into a
/// module-static slot. Returns a friendly `Adapter`;
/// check `result.handle == 0` for failure (the wgpu
/// callback either never fired or reported an error).
///
/// `surfaceHandle` is the `WGPUSurface` the chosen adapter
/// must be able to present to; pass `surface.handle as i64`
/// (or `0` to skip the surface compatibility check). The
/// raw `i64` instead of `Surface` keeps this file out of the
/// surface → adapter → device dependency arc.
///
/// This is a top-level function — NOT a method on `Instance`
/// — because hosting it on `Instance` would close the class
/// DAG into a cycle (see file header).
pub fn requestAdapter(
    inst:          Instance,
    surfaceHandle: i64,
    powerPref:     PowerPreference,
    backend:       BackendType
): Adapter {
    let instH = inst.handle as WGPUInstance
    let opts = WGPURequestAdapterOptions {
        nextInChain:          0 as *void,
        featureLevel:         FeatureLevel.undefined,
        powerPreference:      powerPref,
        forceFallbackAdapter: 0 as u32,
        backendType:          backend,
        compatibleSurface:    surfaceHandle as *void,
    }
    let info = WGPURequestAdapterCallbackInfo {
        nextInChain: 0 as *void,
        mode:        CallbackMode.allowProcessEvents,
        callback:    _wgpuOnRequestAdapter,
        userdata1:   0 as *void,
        userdata2:   0 as *void,
    }
    _wgpuRequestAdapterOut[0] = 0
    wgpuInstanceRequestAdapter(instH, (&opts) as *void, info)
    // Bounded spin — wgpu typically fires on the first
    // pump; cap at 256 iterations so a stuck callback
    // can't deadlock the caller.
    let i: i64 = 0
    while _wgpuRequestAdapterOut[0] == 0 && i < 256 {
        wgpuInstanceProcessEvents(instH)
        i = i + 1
    }
    new Adapter(_wgpuRequestAdapterOut[0] as WGPUAdapter)
}

/// Wraps `WGPUAdapter`. `requestDevice` is the synchronous
/// callback-pumped wrapper; the raw `wgpuAdapterRequestDevice`
/// stays available via the umbrella module.
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

    /// Synchronously request the default device for this
    /// adapter. Internally pumps `wgpuInstanceProcessEvents`
    /// until the wgpu callback fires. Returns a friendly
    /// `Device`; check `result.handle == 0` for failure.
    pub requestDevice(inst: Instance): Device {
        let instH = inst.handle as WGPUInstance
        let dInfo = WGPURequestDeviceCallbackInfo {
            nextInChain: 0 as *void,
            mode:        CallbackMode.allowProcessEvents,
            callback:    _wgpuOnRequestDevice,
            userdata1:   0 as *void,
            userdata2:   0 as *void,
        }
        _wgpuRequestDeviceOut[0] = 0
        wgpuAdapterRequestDevice(this.handle as WGPUAdapter, 0 as *void, dInfo)
        let i: i64 = 0
        while _wgpuRequestDeviceOut[0] == 0 && i < 256 {
            wgpuInstanceProcessEvents(instH)
            i = i + 1
        }
        new Device(_wgpuRequestDeviceOut[0] as WGPUDevice)
    }
}
}
