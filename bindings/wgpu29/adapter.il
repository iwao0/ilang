/// `WGPUAdapter` + `wgpuInstanceRequestAdapter` async plumbing.
///
/// `compatibleSurface` in `WGPURequestAdapterOptions` is typed
/// as `*void` to break the adapter ↔ surface module cycle —
/// pass `surface.handle as *void` (or `0 as *void` to skip the
/// surface compatibility check).

use core {
    WGPUStringView, WGPULimits, FeatureName,
    WGPUInstance, WGPUAdapter, WGPUDevice,
    WGPUAdapterInfo, WGPUSupportedFeatures,
    AdapterType, BackendType,
    wgpuAdapterInfoFreeMembers, wgpuInstanceProcessEvents,
    CallbackMode,
}
use device {
    Device, WGPURequestDeviceCallbackInfo, wgpuAdapterRequestDevice,
}
use std.ffi { cstrFromString }

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

// ─── sync wrapper — module-level OUT slot + C callback ───────
//
// Backs `Adapter.requestDevice`. Same pattern as the
// `requestAdapter` slot in `instance.il`: reset, post, pump
// `wgpuInstanceProcessEvents` until the slot is filled, return.
let _wgpuRequestDeviceOut: i64[] = [0]

@extern(C) {
    pub fn _wgpuOnRequestDevice(
        status: i32, device: i64, msgData: i64, msgLen: i64,
        ud1: i64, ud2: i64
    ) {
        _wgpuRequestDeviceOut[0] = device
    }
}

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

        /// Synchronously request the default device for this
        /// adapter. Internally pumps `wgpuInstanceProcessEvents`
        /// until the wgpu callback fires. Returns a friendly
        /// `Device`; check `result.handle == 0` for failure.
        ///
        /// `instHandle` is the owning `WGPUInstance` (used to
        /// pump events). Passed as a raw handle to avoid
        /// `adapter.il → instance.il`, which would cycle back
        /// through the new `instance.il → adapter.il` edge for
        /// `Instance.requestAdapter`.
        pub requestDevice(instHandle: WGPUInstance): Device {
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
                wgpuInstanceProcessEvents(instHandle)
                i = i + 1
            }
            new Device(_wgpuRequestDeviceOut[0] as WGPUDevice)
        }
    }
}
