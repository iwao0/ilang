/// `WGPUQueue` ops — submit + queue-write data uploads + the
/// friendly `Queue` wrapper class. The `WGPUQueue` handle itself
/// lives in `device.il` next to `WGPUDevice`.

use core {
    WGPUQueue, WGPUExtent3D, WGPUStringView, CallbackMode,
}
use buffer { WGPUBuffer }
use texture { WGPUTexelCopyTextureInfo, WGPUTexelCopyBufferLayout }
use command { WGPUCommandBuffer }

pub enum QueueWorkDoneStatus: u32 {
    success           = 0x01
    callbackCancelled = 0x02
    error             = 0x03
}

// The `Queue.onSubmittedWorkDone` sync slot + callback live in
// `queue_class.il`.

@extern(C, "wgpu_native") {
    // `commands` is a `WGPUCommandBuffer const*` (array). Pass a
    // 1-element `WGPUCommandBuffer[]` (the array → pointer
    // coercion).
    @lib pub fn wgpuQueueSubmit(
        queue: WGPUQueue, commandCount: u64, commands: *WGPUCommandBuffer
    )

    // `wgpuQueueWriteBuffer` is the no-staging fast path for
    // small / single-shot uploads (uniform writes, per-frame
    // transform matrices). For larger data prefer `MapAsync` +
    // a CopyDst buffer.
    @lib pub fn wgpuQueueWriteBuffer(
        queue: WGPUQueue, buffer: WGPUBuffer, bufferOffset: u64,
        data: *const void, size: u64
    )
    // CPU → texture upload. `dataLayout.bytesPerRow` must be a
    // multiple of 256 (same WebGPU spec rule as
    // CopyTextureToBuffer); pad rows before calling.
    @lib pub fn wgpuQueueWriteTexture(
        queue:       WGPUQueue,
        destination: *WGPUTexelCopyTextureInfo,
        data:        *const void,
        dataSize:    u64,
        dataLayout:  *WGPUTexelCopyBufferLayout,
        writeSize:   *WGPUExtent3D
    )

    // ─── completion signal + lifetime ─────────────────────────
    //
    // `onSubmittedWorkDone` resolves when every previously
    // submitted command buffer has finished executing. Callback
    //   void(WGPUQueueWorkDoneStatus status, WGPUStringView msg,
    //        void* ud1, void* ud2)
    // → (status: i32, msg.data: i64, msg.len: i64, ud1: i64, ud2: i64).
    pub struct WGPUQueueWorkDoneCallbackInfo {
        nextInChain: *void
        mode:        CallbackMode
        callback:    fn(i32, i64, i64, i64, i64)
        userdata1:   *void
        userdata2:   *void
    }
    @lib pub fn wgpuQueueOnSubmittedWorkDone(
        queue: WGPUQueue, callbackInfo: WGPUQueueWorkDoneCallbackInfo
    ): u64                                     // WGPUFuture
    @lib pub fn wgpuQueueSetLabel(queue: WGPUQueue, label: WGPUStringView)
    @lib pub fn wgpuQueueAddRef(queue: WGPUQueue)
    @lib pub fn wgpuQueueRelease(queue: WGPUQueue)

    // wgpu-native extension. Same shape as `wgpuQueueSubmit` but
    // returns a `WGPUSubmissionIndex` (u64) that can be passed
    // to `wgpuDevicePoll(..., submissionIndex)` to wait on that
    // specific submission rather than every queued one.
    @lib pub fn wgpuQueueSubmitForIndex(
        queue: WGPUQueue, commandCount: u64, commands: *WGPUCommandBuffer
    ): u64

    // wgpu-native extension. Conversion factor (ns per timestamp
    // tick) for samples written by `WriteTimestamp` /
    // `ResolveQuerySet` on a Timestamp QuerySet. Multiply each
    // u64 sample by this to get nanoseconds. Returns 0.0 when
    // the timestamp feature isn't enabled.
    @lib pub fn wgpuQueueGetTimestampPeriod(queue: WGPUQueue): f32
    // wgpu-native macOS extension. Returns the underlying Metal
    // `id<MTLCommandQueue>`; null on non-Metal backends.
    @lib pub fn wgpuQueueGetNativeMetalCommandQueue(queue: WGPUQueue): *void
}

// `Queue` wrapper class lives in `queue_class.il`.
