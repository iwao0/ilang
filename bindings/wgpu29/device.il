/// Adapter→device promotion + queue retrieval + the wgpu-native
/// `DevicePoll` extension + the friendly `Device` class that
/// turns a raw `WGPUDevice` into a method-chained API.
///
/// `WGPUDevice` and `WGPUQueue` handles live in `core.il` (so
/// downstream feature files can declare `wgpuDeviceCreate*(...)`
/// without cycling back through this file's Device class).

use core {
    WGPUDevice, WGPUQueue, WGPUStringView,
    WGPULimits,
    WGPUAdapter, WGPUAdapterInfo, WGPUSupportedFeatures,
    CallbackMode,
}

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
}

// `Device`, `Limits`, `AdapterInfo` wrapper classes live in
// `device_class.il`.
