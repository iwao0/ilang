/// `WGPUDevice` + `WGPUQueue` handles + adapter→device
/// promotion + queue retrieval + the wgpu-native `DevicePoll`
/// extension. Raw layer only — the friendly `Device` class lives
/// in `pipeline.il` (it needs to return Shader / Pipeline
/// instances, which are declared downstream).

use adapter { WGPUAdapter }

@extern(C, "wgpu_native") {
    @handle pub struct WGPUDevice {}
    @handle pub struct WGPUQueue {}

    // Same layout as RequestAdapterCallbackInfo; callback is
    //   void(WGPURequestDeviceStatus, WGPUDevice, WGPUStringView,
    //        void*, void*)
    pub struct WGPURequestDeviceCallbackInfo {
        nextInChain: *void
        mode:        u32
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
}
